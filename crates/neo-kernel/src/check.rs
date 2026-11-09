use std::rc::Rc;

use crate::conv::{conv, conv_ty};
use crate::core::{Branch, RcTm, Tm};
use crate::env::{CtorInfo, Globals, Sort};
use crate::eval::{apply_pi, eval, eval_closure, force, vapp};
use crate::quote::quote;
use crate::value::*;
use neo_common::error::{Error, Result};

#[derive(Clone, Debug)]
pub struct Ctx {
    pub locals: Vec<(String, V)>,
    pub env: Env,
}

impl Default for Ctx {
    fn default() -> Self {
        Self::new()
    }
}

impl Ctx {
    pub fn new() -> Self {
        Self {
            locals: Vec::new(),
            env: Vec::new(),
        }
    }

    pub fn level(&self) -> usize {
        self.locals.len()
    }

    pub fn push(&mut self, name: impl Into<String>, ty: V) -> V {
        let level = self.locals.len();
        let var = vvar(level);
        self.locals.push((name.into(), ty));
        self.env.push(var.clone());
        var
    }

    pub fn pop(&mut self) {
        self.locals.pop();
        self.env.pop();
    }

    pub fn lookup_index(&self, name: &str) -> Option<usize> {
        for (i, (n, _)) in self.locals.iter().enumerate().rev() {
            if n == name {
                return Some(self.locals.len() - 1 - i);
            }
        }
        None
    }

    pub fn local_ty(&self, index: usize) -> Option<&V> {
        self.locals
            .get(self.locals.len().checked_sub(1 + index)?)
            .map(|(_, t)| t)
    }
}

pub fn infer(globals: &Globals, ctx: &mut Ctx, tm: &RcTm) -> Result<V> {
    use Tm::*;
    match &**tm {
        Var(i) => ctx
            .local_ty(*i)
            .cloned()
            .ok_or_else(|| Error::new(format!("unbound variable index {}", i))),
        Const(name) => {
            let ty = globals
                .get_ty(name)
                .ok_or_else(|| Error::new(format!("unknown name `{}`", name)))?;
            Ok(eval(globals, &Vec::new(), ty))
        }
        Type(u) => Ok(vtype(u + 1)),
        Prop => Ok(vtype(0)),
        Pi { dom, cod, name } => {
            let dom_sort = infer_sort(globals, ctx, dom)?;
            let dom_v = eval(globals, &ctx.env, dom);
            ctx.push(name.clone(), dom_v);
            let cod_sort = infer_sort(globals, ctx, cod);
            ctx.pop();
            let cod_sort = cod_sort?;
            let result = if cod_sort == Sort::Prop {
                Sort::Prop
            } else {
                Sort::Type(dom_sort.level().max(cod_sort.level()))
            };
            Ok(sort_to_val(result))
        }
        Lam { name, dom, body } => {
            infer_sort(globals, ctx, dom)?;
            let dom_v = eval(globals, &ctx.env, dom);
            let outer_env = ctx.env.clone();
            ctx.push(name.clone(), dom_v.clone());
            let body_ty = infer(globals, ctx, body)?;
            let cod_tm = quote(globals, ctx.level(), &body_ty);
            ctx.pop();
            Ok(vpi(
                dom_v,
                Closure {
                    dom: None,
                    env: outer_env,
                    body: cod_tm,
                },
            ))
        }
        App(f, a) => {
            let f_ty = infer(globals, ctx, f)?;
            let f_ty = force(globals, f_ty);
            let (dom, closure) = match &*f_ty {
                Val::Pi(dom, closure) => (dom.clone(), closure.clone()),
                _ => return Err(Error::new("function application: expected a function")),
            };
            check(globals, ctx, a, &dom)?;
            let arg = eval(globals, &ctx.env, a);
            Ok(eval_closure(globals, &closure, arg))
        }
        Ind(name, args) => {
            let info = globals
                .get_enum(name)
                .ok_or_else(|| Error::new(format!("unknown enum `{}`", name)))?;
            if args.len() != info.param_tys.len() {
                return Err(Error::new(format!(
                    "`{}` expects {} parameters, got {}",
                    name,
                    info.param_tys.len(),
                    args.len()
                )));
            }
            let mut local_env: Env = Vec::new();
            for (arg, param_ty) in args.iter().zip(info.param_tys.iter()) {
                let dom = eval(globals, &local_env, param_ty);
                check(globals, ctx, arg, &dom)?;
                local_env.push(eval(globals, &ctx.env, arg));
            }
            Ok(sort_to_val(info.sort))
        }
        Ctor(enum_name, index, args) => {
            let info = globals.get_ctor_info(enum_name, *index).ok_or_else(|| {
                Error::new(format!("unknown constructor {}#{}", enum_name, index))
            })?;
            if args.len() != info.arity() {
                return Err(Error::new(format!(
                    "constructor `{}` expects {} arguments, got {}",
                    info.name,
                    info.arity(),
                    args.len()
                )));
            }
            let mut ty = eval(globals, &Vec::new(), &info.ty);
            for arg in args {
                let ty_f = force(globals, ty);
                match &*ty_f {
                    Val::Pi(dom, closure) => {
                        check(globals, ctx, arg, dom)?;
                        let av = eval(globals, &ctx.env, arg);
                        ty = eval_closure(globals, closure, av);
                    }
                    _ => return Err(Error::new("constructor has too many arguments")),
                }
            }
            Ok(ty)
        }
        Match {
            motive,
            scrut,
            branches,
        } => infer_match(globals, ctx, motive, scrut, branches),
        Eq(ty, lhs, rhs) => {
            infer_sort(globals, ctx, ty)?;
            let ty_v = eval(globals, &ctx.env, ty);
            check(globals, ctx, lhs, &ty_v)?;
            check(globals, ctx, rhs, &ty_v)?;
            Ok(vprop())
        }
        Refl(x) => {
            let ty = infer(globals, ctx, x)?;
            let xv = eval(globals, &ctx.env, x);
            Ok(veq(ty, xv.clone(), xv))
        }
        J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => {
            infer_sort(globals, ctx, ty)?;
            let proof_ty = infer(globals, ctx, proof)?;
            let proof_ty = force(globals, proof_ty);
            let (eq_ty, eq_lhs, eq_rhs) = match &*proof_ty {
                Val::Eq(a, l, r) => (a.clone(), l.clone(), r.clone()),
                _ => {
                    return Err(Error::new(
                        "equality elimination: expected a proof of equality",
                    ));
                }
            };
            let ty_v = eval(globals, &ctx.env, ty);
            let lhs_v = eval(globals, &ctx.env, lhs);
            let rhs_v = eval(globals, &ctx.env, rhs);
            if !conv(globals, ctx.level(), &ty_v, &eq_ty)
                || !conv(globals, ctx.level(), &lhs_v, &eq_lhs)
                || !conv(globals, ctx.level(), &rhs_v, &eq_rhs)
            {
                return Err(Error::new(
                    "equality elimination: equality indices do not match",
                ));
            }
            // The motive is an ordinary type family `(x: ty) -> Sort`. The
            // kernel computes both the branch type and the result by applying
            // it, so no ad-hoc index replacement is trusted.
            let motive_ty = infer(globals, ctx, motive)?;
            let motive_ty = force(globals, motive_ty);
            let motive_dom = match &*motive_ty {
                Val::Pi(dom, _) => dom.clone(),
                _ => {
                    return Err(Error::new("equality elimination: motive is not a function"));
                }
            };
            if !conv(globals, ctx.level(), &motive_dom, &ty_v) {
                return Err(Error::new(
                    "equality elimination: motive domain does not match the equality type",
                ));
            }
            let motive_v = eval(globals, &ctx.env, motive);
            let branch_ty = vapp(globals, motive_v.clone(), lhs_v);
            check(globals, ctx, refl_case, &branch_ty)?;
            Ok(vapp(globals, motive_v, rhs_v))
        }
    }
}

/// The result of refining a constructor's result indices against the type of
/// the scrutinee. The values are in the temporary context obtained by cloning
/// the outer context and pushing the constructor's fields.
pub struct CtorRefinement {
    /// False when the constructor's result indices are disjoint from the
    /// scrutinee's indices (an impossible branch).
    pub possible: bool,
    /// Field types after applying the index solutions.
    pub field_tys: Vec<V>,
    /// Solutions for flexible field variables, in the temporary context.
    pub field_solutions: Vec<Option<V>>,
}

enum UnifyOutcome {
    Ok,
    Impossible,
    Stuck,
}

/// Refine `ctor`'s result indices against `scrut_ty`, where the constructor's
/// parameters have already been instantiated by the scrutinee's parameters.
///
/// This is a first-order pattern unifier: it solves constructor field
/// variables, recognises constructor clashes as impossible branches, and
/// reports anything else as stuck. It is deliberately conservative; the
/// elaborator and the kernel call the same function so they agree on which
/// branches are reachable.
pub fn refine_ctor(
    globals: &Globals,
    ctx: &Ctx,
    ctor: &CtorInfo,
    params: &[V],
    scrut_ty: &V,
) -> Result<CtorRefinement> {
    let mut temp = ctx.clone();
    let mut field_ty = eval(globals, &Vec::new(), &ctor.ty);
    for p in params {
        field_ty = apply_pi(globals, field_ty, p.clone())
            .ok_or_else(|| Error::new("match: constructor parameter mismatch"))?;
    }

    let mut field_tys = Vec::with_capacity(ctor.num_fields);
    let mut field_levels = Vec::with_capacity(ctor.num_fields);
    for j in 0..ctor.num_fields {
        let ft = force(globals, field_ty);
        let (dom, closure) = match &*ft {
            Val::Pi(dom, closure) => (dom.clone(), closure.clone()),
            _ => return Err(Error::new("match: constructor field mismatch")),
        };
        let level = temp.level();
        temp.push(format!("f{}", j), dom.clone());
        field_tys.push(dom);
        field_levels.push(level);
        field_ty = eval_closure(globals, &closure, vvar(level));
    }

    let result_ty = force(globals, field_ty);
    let scrut_ty = force(globals, scrut_ty.clone());
    let mut solutions = std::collections::HashMap::new();
    let outcome = unify_indices(
        globals,
        temp.level(),
        &field_levels,
        &mut solutions,
        &result_ty,
        &scrut_ty,
    );

    match outcome {
        UnifyOutcome::Impossible => Ok(CtorRefinement {
            possible: false,
            field_tys,
            field_solutions: vec![None; ctor.num_fields],
        }),
        UnifyOutcome::Stuck => {
            let got = crate::pretty::show_val(globals, temp.level(), &result_ty);
            let want = crate::pretty::show_val(globals, temp.level(), &scrut_ty);
            Err(Error::new(format!(
                "match: cannot refine constructor `{}` result `{}` against scrutinee type `{}`",
                ctor.name, got, want
            )))
        }
        UnifyOutcome::Ok => {
            let result_ty = apply_solutions(globals, &temp, &field_levels, &solutions, &result_ty);
            if !conv(globals, temp.level(), &result_ty, &scrut_ty) {
                let got = crate::pretty::show_val(globals, temp.level(), &result_ty);
                let want = crate::pretty::show_val(globals, temp.level(), &scrut_ty);
                return Err(Error::new(format!(
                    "match: refined constructor `{}` result `{}` does not match scrutinee type `{}`",
                    ctor.name, got, want
                )));
            }
            let field_tys = field_tys
                .iter()
                .map(|ty| apply_solutions(globals, &temp, &field_levels, &solutions, ty))
                .collect();
            let field_solutions = field_levels
                .iter()
                .map(|level| {
                    solutions.get(level).map(|value| {
                        apply_solutions(globals, &temp, &field_levels, &solutions, value)
                    })
                })
                .collect();
            Ok(CtorRefinement {
                possible: true,
                field_tys,
                field_solutions,
            })
        }
    }
}

fn unify_indices(
    globals: &Globals,
    level: usize,
    flexible: &[usize],
    solutions: &mut std::collections::HashMap<usize, V>,
    a: &V,
    b: &V,
) -> UnifyOutcome {
    if conv(globals, level, a, b) {
        return UnifyOutcome::Ok;
    }
    let a = force(globals, a.clone());
    let b = force(globals, b.clone());

    if let Val::Neutral(na) = &*a
        && let Neutral::Var(i) = &**na
        && flexible.contains(i)
    {
        return bind_flexible(globals, level, flexible, solutions, *i, &b);
    }
    if let Val::Neutral(nb) = &*b
        && let Neutral::Var(i) = &**nb
        && flexible.contains(i)
    {
        return bind_flexible(globals, level, flexible, solutions, *i, &a);
    }

    match (&*a, &*b) {
        (Val::Ctor(n1, i1, args1), Val::Ctor(n2, i2, args2)) => {
            if n1 != n2 || i1 != i2 {
                UnifyOutcome::Impossible
            } else {
                unify_lists(globals, level, flexible, solutions, args1, args2)
            }
        }
        (Val::Ind(n1, args1), Val::Ind(n2, args2)) => {
            if n1 != n2 {
                UnifyOutcome::Impossible
            } else {
                unify_lists(globals, level, flexible, solutions, args1, args2)
            }
        }
        (Val::Type(u), Val::Type(v)) => {
            if u == v {
                UnifyOutcome::Ok
            } else {
                UnifyOutcome::Stuck
            }
        }
        (Val::Prop, Val::Prop) => UnifyOutcome::Ok,
        (Val::Eq(t1, l1, r1), Val::Eq(t2, l2, r2)) => unify_lists(
            globals,
            level,
            flexible,
            solutions,
            &[t1.clone(), l1.clone(), r1.clone()],
            &[t2.clone(), l2.clone(), r2.clone()],
        ),
        (Val::Refl(x), Val::Refl(y)) => unify_indices(globals, level, flexible, solutions, x, y),
        (Val::Neutral(n1), Val::Neutral(n2)) => match (&**n1, &**n2) {
            (Neutral::Var(i), Neutral::Var(j)) if i == j => UnifyOutcome::Ok,
            (Neutral::Global(x), Neutral::Global(y)) if x == y => UnifyOutcome::Ok,
            (Neutral::App(f1, a1), Neutral::App(f2, a2)) => {
                let f = unify_indices(
                    globals,
                    level,
                    flexible,
                    solutions,
                    &vneutral((**f1).clone()),
                    &vneutral((**f2).clone()),
                );
                match f {
                    UnifyOutcome::Impossible => UnifyOutcome::Impossible,
                    UnifyOutcome::Stuck => UnifyOutcome::Stuck,
                    UnifyOutcome::Ok => unify_indices(globals, level, flexible, solutions, a1, a2),
                }
            }
            _ => UnifyOutcome::Stuck,
        },
        _ => UnifyOutcome::Stuck,
    }
}

fn unify_lists(
    globals: &Globals,
    level: usize,
    flexible: &[usize],
    solutions: &mut std::collections::HashMap<usize, V>,
    a: &[V],
    b: &[V],
) -> UnifyOutcome {
    if a.len() != b.len() {
        return UnifyOutcome::Stuck;
    }
    let mut stuck = false;
    for (x, y) in a.iter().zip(b.iter()) {
        match unify_indices(globals, level, flexible, solutions, x, y) {
            UnifyOutcome::Impossible => return UnifyOutcome::Impossible,
            UnifyOutcome::Stuck => stuck = true,
            UnifyOutcome::Ok => {}
        }
    }
    if stuck {
        UnifyOutcome::Stuck
    } else {
        UnifyOutcome::Ok
    }
}

fn bind_flexible(
    globals: &Globals,
    level: usize,
    flexible: &[usize],
    solutions: &mut std::collections::HashMap<usize, V>,
    var: usize,
    value: &V,
) -> UnifyOutcome {
    if let Some(existing) = solutions.get(&var).cloned() {
        return unify_indices(globals, level, flexible, solutions, &existing, value);
    }
    if occurs(globals, var, value) {
        return UnifyOutcome::Stuck;
    }
    solutions.insert(var, value.clone());
    UnifyOutcome::Ok
}

fn occurs(globals: &Globals, var: usize, value: &V) -> bool {
    let value = force(globals, value.clone());
    match &*value {
        Val::Neutral(n) => match &**n {
            Neutral::Var(i) => *i == var,
            Neutral::App(f, a) => {
                occurs(globals, var, &vneutral((**f).clone())) || occurs(globals, var, a)
            }
            Neutral::Global(_) => false,
            Neutral::Match { .. } | Neutral::J { .. } => true,
        },
        Val::Ind(_, args) | Val::Ctor(_, _, args) => args.iter().any(|a| occurs(globals, var, a)),
        Val::Eq(a, b, c) => {
            occurs(globals, var, a) || occurs(globals, var, b) || occurs(globals, var, c)
        }
        Val::Refl(a) => occurs(globals, var, a),
        Val::Pi(..) | Val::Lam(_) => true,
        Val::Type(_) | Val::Prop | Val::Glob(_) => false,
    }
}

fn apply_solutions(
    globals: &Globals,
    temp: &Ctx,
    field_levels: &[usize],
    solutions: &std::collections::HashMap<usize, V>,
    value: &V,
) -> V {
    if solutions.is_empty() {
        return value.clone();
    }
    let level = temp.level();
    let mut tm = quote(globals, level, value);
    for _ in 0..=field_levels.len() {
        for field_level in field_levels {
            if let Some(solution) = solutions.get(field_level) {
                let index = level - 1 - *field_level;
                let replacement = quote(globals, level, solution);
                tm = crate::core::subst(&tm, index, &replacement);
            }
        }
    }
    eval(globals, &temp.env, &tm)
}

fn infer_match(
    globals: &Globals,
    ctx: &mut Ctx,
    motive: &RcTm,
    scrut: &RcTm,
    branches: &[Branch],
) -> Result<V> {
    let scrut_ty = infer(globals, ctx, scrut)?;
    let scrut_ty = force(globals, scrut_ty);
    let (enum_name, params) = match &*scrut_ty {
        Val::Ind(name, args) => (name.clone(), args.clone()),
        _ => return Err(Error::new("match: scrutinee is not an enum")),
    };
    let info = globals
        .get_enum(&enum_name)
        .ok_or_else(|| Error::new(format!("unknown enum `{}`", enum_name)))?;
    if branches.len() != info.ctors.len() {
        return Err(Error::new("match: not all constructors are covered"));
    }

    let motive_ty = infer(globals, ctx, motive)?;
    let motive_ty = force(globals, motive_ty);
    let motive_dom = match &*motive_ty {
        Val::Pi(dom, _) => dom.clone(),
        _ => return Err(Error::new("match: motive is not a function")),
    };
    if !conv(globals, ctx.level(), &motive_dom, &scrut_ty) {
        return Err(Error::new(
            "match: motive domain does not match scrutinee type",
        ));
    }

    // Large elimination restriction: a `Prop` enum that is not
    // singleton-eliminable may only be eliminated into `Prop`.
    if info.sort == Sort::Prop
        && !info.prop_eliminable
        && let Val::Pi(dom, _) = &*motive_ty
    {
        let created = ctx.push("_motive", dom.clone());
        let applied = crate::core::app(motive.clone(), crate::core::var(0));
        let result = infer(globals, ctx, &applied)
            .and_then(|ty| sort_of_val_in(globals, ctx, &ty))
            .map(|sort| matches!(sort, Sort::Type(_)));
        ctx.pop();
        let _ = created;
        if result? {
            return Err(Error::new(
                "match: cannot eliminate this proposition into a type",
            ));
        }
    }

    let motive_v = eval(globals, &ctx.env, motive);
    let scrut_v = eval(globals, &ctx.env, scrut);

    for (i, branch) in branches.iter().enumerate() {
        let ctor = &info.ctors[i];
        if branch.ctor != ctor.name {
            return Err(Error::new(format!(
                "match: expected branch for `{}`, found `{}`",
                ctor.name, branch.ctor
            )));
        }
        if branch.binders != ctor.num_fields {
            return Err(Error::new(format!(
                "match: constructor `{}` expects {} binders, got {}",
                ctor.name, ctor.num_fields, branch.binders
            )));
        }

        let refinement = refine_ctor(globals, ctx, ctor, &params, &scrut_ty)?;
        if !refinement.possible {
            // The constructor's result indices are disjoint from the
            // scrutinee type, so this branch is unreachable. Its body is not
            // checked; the elaborator marks it so evaluation and code
            // generation skip it.
            continue;
        }
        if branch.impossible {
            return Err(Error::new(format!(
                "match: branch for `{}` is marked impossible but is reachable",
                ctor.name
            )));
        }

        let mut field_vals = Vec::with_capacity(ctor.num_fields);
        for (j, ty) in refinement.field_tys.iter().enumerate() {
            let fresh = ctx.push(format!("f{}", j), ty.clone());
            match &refinement.field_solutions[j] {
                Some(solution) => field_vals.push(solution.clone()),
                None => field_vals.push(fresh),
            }
        }
        let mut ctor_args = params.clone();
        ctor_args.extend(field_vals);
        let ctor_v = vctor(enum_name.clone(), i, ctor_args);
        let branch_ty = vapp(globals, motive_v.clone(), ctor_v);
        let result = check(globals, ctx, &branch.body, &branch_ty);
        for _ in 0..ctor.num_fields {
            ctx.pop();
        }
        result?;
    }

    Ok(vapp(globals, motive_v, scrut_v))
}

pub fn check(globals: &Globals, ctx: &mut Ctx, tm: &RcTm, expected: &V) -> Result<()> {
    use Tm::*;
    match &**tm {
        Lam { name, dom, body } => {
            let expected = force(globals, expected.clone());
            let (dom_v, closure) = match &*expected {
                Val::Pi(dom, closure) => (dom.clone(), closure.clone()),
                _ => return Err(Error::new("lambda: expected a function type")),
            };
            infer_sort(globals, ctx, dom)?;
            let dom_tm_v = eval(globals, &ctx.env, dom);
            if !conv(globals, ctx.level(), &dom_tm_v, &dom_v) {
                return Err(Error::new("lambda: domain does not match expected type"));
            }
            let fresh = ctx.push(name.clone(), dom_v);
            let cod = eval_closure(globals, &closure, fresh);
            let result = check(globals, ctx, body, &cod);
            ctx.pop();
            result
        }
        Refl(x) => {
            let expected = force(globals, expected.clone());
            let (ty, lhs, rhs) = match &*expected {
                Val::Eq(ty, lhs, rhs) => (ty.clone(), lhs.clone(), rhs.clone()),
                _ => return Err(Error::new("refl: expected an equality type")),
            };
            check(globals, ctx, x, &ty)?;
            let xv = eval(globals, &ctx.env, x);
            if conv(globals, ctx.level(), &xv, &lhs) && conv(globals, ctx.level(), &xv, &rhs) {
                Ok(())
            } else {
                Err(Error::new("refl: the two sides are not convertible"))
            }
        }
        _ => {
            let inferred = infer(globals, ctx, tm)?;
            if conv_ty(globals, ctx.level(), &inferred, expected) {
                Ok(())
            } else {
                Err(Error::new("type mismatch"))
            }
        }
    }
}

pub fn infer_sort(globals: &Globals, ctx: &mut Ctx, tm: &RcTm) -> Result<Sort> {
    let ty = infer(globals, ctx, tm)?;
    sort_of_val_in(globals, ctx, &ty)
}

/// Compute the sort of a type value, using the context to resolve neutral
/// variables and applications. This is essential for dependent types such as
/// `fn(x: T) -> Prop`, where `T` is a neutral variable.
pub fn sort_of_val_in(globals: &Globals, ctx: &Ctx, v: &V) -> Result<Sort> {
    let v = force(globals, v.clone());
    match &*v {
        Val::Type(u) => Ok(Sort::Type(*u)),
        Val::Prop => Ok(Sort::Prop),
        Val::Ind(name, _) => globals
            .sort_of_enum(name)
            .ok_or_else(|| Error::new(format!("unknown enum `{}`", name))),
        Val::Eq(_, _, _) => Ok(Sort::Prop),
        Val::Pi(dom, closure) => {
            let dom_sort = sort_of_val_in(globals, ctx, dom)?;
            let mut inner = ctx.clone();
            let fresh = inner.push("_", dom.clone());
            let cod = eval_closure(globals, closure, fresh);
            let cod_sort = sort_of_val_in(globals, &inner, &cod)?;
            if cod_sort == Sort::Prop {
                Ok(Sort::Prop)
            } else {
                Ok(Sort::Type(dom_sort.level().max(cod_sort.level())))
            }
        }
        Val::Neutral(n) => {
            let ty = neutral_type_in(globals, ctx, n)?;
            sort_of_val_in(globals, ctx, &ty)
        }
        _ => Err(Error::new("expected a type")),
    }
}

fn neutral_type_in(globals: &Globals, ctx: &Ctx, n: &Neutral) -> Result<V> {
    match n {
        Neutral::Var(i) => ctx
            .locals
            .get(*i)
            .map(|(_, ty)| ty.clone())
            .ok_or_else(|| Error::new(format!("unbound variable level {}", i))),
        Neutral::App(f, a) => {
            let f_ty = neutral_type_in(globals, ctx, f)?;
            let f_ty = force(globals, f_ty);
            match &*f_ty {
                Val::Pi(_, closure) => Ok(eval_closure(globals, closure, a.clone())),
                _ => Err(Error::new("expected a type")),
            }
        }
        _ => Err(Error::new("expected a type")),
    }
}

pub fn sort_of_val(globals: &Globals, v: &V) -> Result<Sort> {
    sort_of_val_in(globals, &Ctx::new(), v)
}

/// Re-validate every enum declaration from the stored data. This makes the
/// kernel authoritative for positivity, field universes, result indices, and
/// the singleton-elimination flag, rather than trusting the elaborator.
pub fn check_enums(globals: &Globals) -> Result<()> {
    for info in globals.enums.values() {
        let mut params_ctx = Ctx::new();
        for (name, ty) in info.param_names.iter().zip(info.param_tys.iter()) {
            infer_sort(globals, &mut params_ctx, ty).map_err(|e| {
                Error::new(format!(
                    "kernel rejected enum `{}`: invalid parameter: {}",
                    info.name, e.message
                ))
            })?;
            let v = eval(globals, &params_ctx.env, ty);
            params_ctx.push(name.clone(), v);
        }

        let mut singleton_ok = Vec::new();
        for ctor in &info.ctors {
            let mut ctx = Ctx::new();
            let mut cur = &ctor.ty;
            for _ in 0..info.param_names.len() {
                match &**cur {
                    Tm::Pi { dom, cod, .. } => {
                        infer_sort(globals, &mut ctx, dom).map_err(|e| {
                            Error::new(format!(
                                "kernel rejected constructor `{}`: bad parameter: {}",
                                ctor.name, e.message
                            ))
                        })?;
                        let v = eval(globals, &ctx.env, dom);
                        ctx.push("_", v);
                        cur = cod;
                    }
                    _ => {
                        return Err(Error::new(format!(
                            "kernel rejected constructor `{}`: missing parameter",
                            ctor.name
                        )));
                    }
                }
            }

            let mut all_fields_prop = true;
            for _ in 0..ctor.num_fields {
                match &**cur {
                    Tm::Pi { dom, cod, .. } => {
                        crate::positivity::check_field(globals, &info.name, dom).map_err(|e| {
                            Error::new(format!(
                                "kernel rejected constructor `{}`: {}",
                                ctor.name, e.message
                            ))
                        })?;
                        let field_sort = infer_sort(globals, &mut ctx, dom).map_err(|e| {
                            Error::new(format!(
                                "kernel rejected constructor `{}`: bad field: {}",
                                ctor.name, e.message
                            ))
                        })?;
                        if let Sort::Type(u) = info.sort
                            && field_sort.level() > u
                        {
                            return Err(Error::new(format!(
                                "kernel rejected constructor `{}`: field is larger than the enum",
                                ctor.name
                            )));
                        }
                        all_fields_prop &= field_sort == Sort::Prop;
                        let v = eval(globals, &ctx.env, dom);
                        ctx.push("_", v);
                        cur = cod;
                    }
                    _ => {
                        return Err(Error::new(format!(
                            "kernel rejected constructor `{}`: missing field",
                            ctor.name
                        )));
                    }
                }
            }

            match &**cur {
                Tm::Ind(name, args) if name == &info.name => {
                    if args.len() != info.param_names.len() {
                        return Err(Error::new(format!(
                            "kernel rejected constructor `{}`: wrong number of result indices",
                            ctor.name
                        )));
                    }
                    for arg in args {
                        crate::positivity::check_index(globals, &info.name, arg).map_err(|e| {
                            Error::new(format!(
                                "kernel rejected constructor `{}`: {}",
                                ctor.name, e.message
                            ))
                        })?;
                        let index_sort = infer_sort(globals, &mut ctx, arg).map_err(|e| {
                            Error::new(format!(
                                "kernel rejected constructor `{}`: bad index: {}",
                                ctor.name, e.message
                            ))
                        })?;
                        if let Sort::Type(u) = info.sort
                            && index_sort.level() > u
                        {
                            return Err(Error::new(format!(
                                "kernel rejected constructor `{}`: index is larger than the enum",
                                ctor.name
                            )));
                        }
                    }
                }
                _ => {
                    return Err(Error::new(format!(
                        "kernel rejected constructor `{}`: result is not `{}`",
                        ctor.name, info.name
                    )));
                }
            }
            singleton_ok.push(all_fields_prop);
        }

        let expected = info.sort == Sort::Prop
            && (singleton_ok.is_empty() || (singleton_ok.len() == 1 && singleton_ok[0]));
        if expected != info.prop_eliminable {
            return Err(Error::new(format!(
                "kernel rejected enum `{}`: inconsistent elimination flag",
                info.name
            )));
        }
    }
    Ok(())
}

fn sort_to_val(s: Sort) -> V {
    match s {
        Sort::Prop => vprop(),
        Sort::Type(u) => vtype(u),
    }
}

pub fn sort_value(s: Sort) -> V {
    sort_to_val(s)
}

pub fn normalize(globals: &Globals, tm: &RcTm) -> RcTm {
    let v = eval(globals, &Vec::new(), tm);
    quote(globals, 0, &v)
}

pub fn conv_terms(globals: &Globals, a: &RcTm, b: &RcTm) -> bool {
    let av = eval(globals, &Vec::new(), a);
    let bv = eval(globals, &Vec::new(), b);
    conv(globals, 0, &av, &bv)
}

pub fn eqtm(ty: RcTm, lhs: RcTm, rhs: RcTm) -> RcTm {
    Rc::new(Tm::Eq(ty, lhs, rhs))
}
