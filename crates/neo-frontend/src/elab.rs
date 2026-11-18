use crate::check::{self, Ctx};
use crate::conv::{conv, conv_ty};
use crate::core::{
    Branch, PrimOp, PrimTy, RcTm, Tm, abstract_term, app, const_, eq, ind, lam, pi, pi_with,
    prim_lit, prim_op, prim_ty, rc, shift, var,
};
use crate::env::{CtorInfo, EnumInfo, GlobalDef, Globals, Sort};
use crate::eval::{apply_pi, eval, eval_closure, force, vapp};
use crate::quote::quote;
use crate::surface::*;
use crate::value::*;
use neo_common::error::{Error, Result, Span};

/// Elaborate and register every declaration in the program.
pub fn elaborate_program(globals: &mut Globals, program: &Program) -> Result<()> {
    for decl in &program.decls {
        match decl {
            Decl::Enum(e) => elab_enum(globals, e)?,
            Decl::Fn(f) => elab_fn(globals, f)?,
        }
    }
    crate::recursion::check_program(globals)?;
    Ok(())
}

fn show_sort(sort: Sort) -> String {
    match sort {
        Sort::Prop => "Prop".to_string(),
        Sort::Type(u) => format!("Type {}", u),
    }
}

/// Names reserved by the primitive layer. Redeclaring one would silently change
/// the meaning of existing programs, so declarations are rejected.
fn reserved_name(name: &str) -> Option<String> {
    if name == PrimTy::U32.name() {
        return Some(format!("`{}` is a primitive type", name));
    }
    if PrimOp::from_name(name).is_some() {
        return Some(format!("`{}` is a primitive operation", name));
    }
    None
}

fn elab_enum(globals: &mut Globals, decl: &EnumDecl) -> Result<()> {
    let result = elab_enum_inner(globals, decl);
    if result.is_err() {
        globals.enums.remove(&decl.name);
    }
    result
}

fn elab_enum_inner(globals: &mut Globals, decl: &EnumDecl) -> Result<()> {
    if globals.enums.contains_key(&decl.name) || globals.defs.contains_key(&decl.name) {
        return Err(Error::at(
            decl.span,
            format!("`{}` is already defined", decl.name),
        ));
    }
    if let Some(message) = reserved_name(&decl.name) {
        return Err(Error::at(decl.span, message));
    }

    let sort = match &decl.sort {
        Some(e) => {
            let mut ctx = Ctx::new();
            let tm = elab_type(globals, &mut ctx, e)?;
            check::infer_sort(globals, &mut Ctx::new(), &tm).map_err(|err| {
                Error::at(e.span(), format!("invalid enum sort: {}", err.message))
            })?;
            let v = force(globals, eval(globals, &ctx.env, &tm));
            match &*v {
                Val::Type(u) => Sort::Type(*u),
                Val::Prop => Sort::Prop,
                _ => {
                    return Err(Error::at(
                        e.span(),
                        "an enum sort must be `Prop` or `Type u`",
                    ));
                }
            }
        }
        None => Sort::Type(0),
    };

    let mut pctx = Ctx::new();
    let mut param_names = Vec::new();
    let mut param_tys = Vec::new();
    for p in &decl.params {
        let tm = elab_type(globals, &mut pctx, &p.ty)?;
        check::infer_sort(globals, &mut pctx, &tm)
            .map_err(|err| Error::at(p.span, format!("invalid enum parameter: {}", err.message)))?;
        let v = eval(globals, &pctx.env, &tm);
        param_names.push(p.name.clone());
        param_tys.push(tm);
        pctx.push(p.name.clone(), v);
    }
    let np = param_names.len();

    // Register the enum before its constructors so that recursive occurrences
    // in field types resolve.
    globals.enums.insert(
        decl.name.clone(),
        EnumInfo {
            name: decl.name.clone(),
            param_names: param_names.clone(),
            param_tys: param_tys.clone(),
            sort,
            ctors: Vec::new(),
            prop_eliminable: false,
        },
    );

    let mut ctor_infos: Vec<CtorInfo> = Vec::new();
    let mut ctor_singleton_ok = Vec::new();
    for (idx, c) in decl.ctors.iter().enumerate() {
        if globals.defs.contains_key(&c.name) || globals.enums.contains_key(&c.name) {
            return Err(Error::at(
                c.span,
                format!("`{}` is already defined", c.name),
            ));
        }
        if ctor_infos.iter().any(|info| info.name == c.name) {
            return Err(Error::at(
                c.span,
                format!("constructor `{}` is defined twice", c.name),
            ));
        }
        if let Some(message) = reserved_name(&c.name) {
            return Err(Error::at(c.span, message));
        }
        let nf = c.fields.len();
        let mut ctx = Ctx::new();
        for (name, ty) in param_names.iter().zip(param_tys.iter()) {
            let v = eval(globals, &ctx.env, ty);
            ctx.push(name.clone(), v);
        }

        let mut field_names = Vec::new();
        let mut field_tms = Vec::new();
        let mut all_fields_prop = true;
        for (j, f) in c.fields.iter().enumerate() {
            let tm = elab_type(globals, &mut ctx, &f.ty)?;
            crate::positivity::check_field(globals, &decl.name, &tm)?;
            let field_sort = check::infer_sort(globals, &mut ctx, &tm).map_err(|e| {
                Error::at(f.span, format!("invalid constructor field: {}", e.message))
            })?;
            if let Sort::Type(u) = sort
                && field_sort.level() > u
            {
                return Err(Error::at(
                    f.span,
                    format!(
                        "constructor field lives in `{}`, but `{}` is declared in `Type {}`; a field may not be larger than its enum",
                        show_sort(field_sort),
                        decl.name,
                        u
                    ),
                ));
            }
            all_fields_prop &= field_sort == Sort::Prop;
            let v = eval(globals, &ctx.env, &tm);
            let fname = f.name.clone().unwrap_or_else(|| format!("_f{}", j));
            field_names.push(fname.clone());
            field_tms.push(tm);
            ctx.push(fname, v);
        }

        let result_tm = match &c.result {
            Some(e) => elab_type(globals, &mut ctx, e)?,
            None => {
                let args = (0..np).map(|j| var(np + nf - 1 - j)).collect();
                ind(decl.name.clone(), args)
            }
        };

        if c.result.is_some() {
            let result_v = force(globals, eval(globals, &ctx.env, &result_tm));
            let valid = match &*result_v {
                Val::Ind(name, args) => name == &decl.name && args.len() == np,
                _ => false,
            };
            if !valid {
                let shown = crate::pretty::show_val(globals, ctx.level(), &result_v);
                return Err(Error::at(
                    c.span,
                    format!(
                        "constructor `{}` must return `{}`, but returns `{}`",
                        c.name, decl.name, shown
                    ),
                ));
            }
            // Conservative positivity and universe checks for the indices.
            if let Val::Ind(_, args) = &*result_v {
                for arg in args {
                    let tm = quote(globals, ctx.level(), arg);
                    crate::positivity::check_index(globals, &decl.name, &tm)
                        .map_err(|e| Error::at(c.span, e.message))?;
                    let index_sort = check::infer_sort(globals, &mut ctx, &tm).map_err(|e| {
                        Error::at(c.span, format!("invalid constructor index: {}", e.message))
                    })?;
                    if let Sort::Type(u) = sort
                        && index_sort.level() > u
                    {
                        return Err(Error::at(
                            c.span,
                            format!(
                                "constructor index lives in `{}`, but `{}` is declared in `Type {}`; an index may not be larger than its enum",
                                show_sort(index_sort),
                                decl.name,
                                u
                            ),
                        ));
                    }
                }
            }
        }

        let mut ty = result_tm;
        for i in (0..nf).rev() {
            ty = pi(field_names[i].clone(), field_tms[i].clone(), ty);
        }
        for i in (0..np).rev() {
            ty = pi_with(param_names[i].clone(), param_tys[i].clone(), ty, true);
        }

        // Sanity-check that the constructor type is well formed and closed.
        let mut empty = Ctx::new();
        check::infer_sort(globals, &mut empty, &ty).map_err(|e| {
            Error::at(
                c.span,
                format!("invalid constructor `{}`: {}", c.name, e.message),
            )
        })?;

        ctor_infos.push(CtorInfo {
            name: c.name.clone(),
            enum_name: decl.name.clone(),
            index: idx,
            num_params: np,
            num_fields: nf,
            ty,
        });
        ctor_singleton_ok.push(all_fields_prop);
    }

    // Singleton elimination: a `Prop` enum can be eliminated into a `Type`
    // only when it has no constructors, or exactly one constructor whose
    // fields are all propositions.
    let prop_eliminable = sort == Sort::Prop
        && (ctor_singleton_ok.is_empty() || (ctor_singleton_ok.len() == 1 && ctor_singleton_ok[0]));
    let enum_info = globals.enums.get_mut(&decl.name).unwrap();
    enum_info.ctors = ctor_infos.clone();
    enum_info.prop_eliminable = prop_eliminable;
    for info in ctor_infos {
        globals.defs.insert(
            info.name.clone(),
            GlobalDef::Ctor {
                enum_name: info.enum_name.clone(),
                index: info.index,
                ty: info.ty.clone(),
                arity: info.arity(),
            },
        );
    }
    Ok(())
}

struct FnTypeInfo {
    ty: RcTm,
    domains: Vec<(String, RcTm, bool)>,
    ret: RcTm,
}

fn build_fn_type(globals: &Globals, decl: &FnDecl) -> Result<FnTypeInfo> {
    let mut ctx = Ctx::new();
    let mut domains = Vec::new();
    for p in &decl.params {
        let tm = elab_type(globals, &mut ctx, &p.ty)?;
        let v = eval(globals, &ctx.env, &tm);
        domains.push((p.name.clone(), tm, p.implicit));
        ctx.push(p.name.clone(), v);
    }
    let ret = elab_type(globals, &mut ctx, &decl.ret)?;
    let mut ty = ret.clone();
    for (name, dom, implicit) in domains.iter().rev() {
        ty = pi_with(name.clone(), dom.clone(), ty, *implicit);
    }
    Ok(FnTypeInfo { ty, domains, ret })
}

fn elab_fn(globals: &mut Globals, decl: &FnDecl) -> Result<()> {
    if globals.defs.contains_key(&decl.name) || globals.enums.contains_key(&decl.name) {
        return Err(Error::at(
            decl.span,
            format!("`{}` is already defined", decl.name),
        ));
    }
    if let Some(message) = reserved_name(&decl.name) {
        return Err(Error::at(decl.span, message));
    }

    let info = build_fn_type(globals, decl)?;
    globals.add_fn_placeholder(&decl.name, info.ty.clone(), info.domains.len());

    let result = elab_fn_body(globals, decl, &info);
    if result.is_err() {
        // Do not leave a bodyless placeholder behind after a failed definition.
        globals.defs.remove(&decl.name);
    }
    result
}

fn elab_fn_body(globals: &mut Globals, decl: &FnDecl, info: &FnTypeInfo) -> Result<()> {
    let mut ctx = Ctx::new();
    for (name, dom, _) in &info.domains {
        let dom_v = eval(globals, &ctx.env, dom);
        ctx.push(name.clone(), dom_v);
    }
    let ret_v = eval(globals, &ctx.env, &info.ret);
    let body_expr = Expr::Block(decl.body.clone());
    let mut body = elab_check(globals, &mut ctx, &body_expr, &ret_v)?;
    for (name, dom, _) in info.domains.iter().rev() {
        body = lam(name.clone(), dom.clone(), body);
    }

    let mut type_ctx = Ctx::new();
    check::infer_sort(globals, &mut type_ctx, &info.ty).map_err(|e| {
        Error::at(
            decl.span,
            format!("kernel rejected the type of `{}`: {}", decl.name, e.message),
        )
    })?;
    let ty_v = eval(globals, &Vec::new(), &info.ty);
    let mut body_ctx = Ctx::new();
    check::check(globals, &mut body_ctx, &body, &ty_v).map_err(|e| {
        Error::at(
            decl.span,
            format!("kernel rejected `{}`: {}", decl.name, e.message),
        )
    })?;

    // The authoritative termination check runs on the elaborated core term,
    // while the definition's own body is still opaque.
    crate::recursion::check_def(globals, &decl.name, &body).map_err(|e| {
        Error::at(
            decl.span,
            format!("kernel rejected `{}`: {}", decl.name, e.message),
        )
    })?;

    globals.set_fn_body(&decl.name, body);
    Ok(())
}

fn elab_type(globals: &Globals, ctx: &mut Ctx, expr: &Expr) -> Result<RcTm> {
    let (tm, ty) = elab_infer(globals, ctx, expr)?;
    check::sort_of_val_in(globals, ctx, &ty)
        .map_err(|e| Error::at(expr.span(), format!("expected a type: {}", e.message)))?;
    Ok(tm)
}

pub fn elab_infer(globals: &Globals, ctx: &mut Ctx, expr: &Expr) -> Result<(RcTm, V)> {
    match expr {
        Expr::Var(name, span) => elab_var(globals, ctx, name, *span),
        Expr::Numeral(n, suffix, span) => elab_numeral_infer(*n, *suffix, *span),
        Expr::Str(s, span) => string_literal(globals, s, *span),
        Expr::Type(u, _) => Ok((rc(Tm::Type(*u)), vtype(u + 1))),
        Expr::Prop(_) => Ok((rc(Tm::Prop), vtype(0))),
        Expr::FnType(binders, body, _) => elab_pi(globals, ctx, binders, body),
        Expr::FnLam(binders, body, span) => elab_lam_infer(globals, ctx, binders, body, *span),
        Expr::App {
            f,
            implicit,
            args,
            span,
        } => {
            if let Expr::Var(n, _) = &**f
                && ctx.lookup_index(n).is_none()
                && globals.is_constructor(n)
            {
                if !implicit.is_empty() {
                    return Err(Error::at(
                        *span,
                        "constructor type arguments are inferred from the expected type; remove the `[...]` arguments",
                    ));
                }
                return elab_ctor_infer(globals, ctx, n, args, *span);
            }
            elab_app(globals, ctx, f, implicit, args, *span)
        }
        Expr::BinOp { op, lhs, rhs, span } => elab_binop_infer(globals, ctx, *op, lhs, rhs, *span),
        Expr::Match(scrut, arms, span) => {
            let (tm, ty) = elab_match(globals, ctx, scrut, arms, None, *span)?;
            Ok((tm, ty))
        }
        Expr::Eq(lhs, rhs, _) => elab_eq(globals, ctx, lhs, rhs),
        Expr::Refl(span) => Err(Error::at(
            *span,
            "`refl` needs an expected equality type; use it where the type is known",
        )),
        Expr::Ascribe(e, ty, _) => {
            let tm_ty = elab_type(globals, ctx, ty)?;
            let v = eval(globals, &ctx.env, &tm_ty);
            let tm = elab_check(globals, ctx, e, &v)?;
            Ok((tm, v))
        }
        Expr::Block(b) => elab_block_infer(globals, ctx, b),
        Expr::Paren(e, _) => elab_infer(globals, ctx, e),
    }
}

fn elab_var(globals: &Globals, ctx: &Ctx, name: &str, span: Span) -> Result<(RcTm, V)> {
    if let Some(idx) = ctx.lookup_index(name) {
        let ty = ctx
            .local_ty(idx)
            .cloned()
            .ok_or_else(|| Error::at(span, format!("unbound variable `{}`", name)))?;
        return Ok((var(idx), ty));
    }
    if name == PrimTy::U32.name() {
        return Ok((prim_ty(PrimTy::U32), vtype(0)));
    }
    if let Some(op) = PrimOp::from_name(name) {
        return Err(Error::at(
            span,
            format!(
                "primitive `{}` must be fully applied to two `U32` arguments",
                op.name()
            ),
        ));
    }
    if let Some(info) = globals.get_enum(name) {
        if info.param_tys.is_empty() {
            let ty = check::sort_value(info.sort);
            return Ok((ind(name, Vec::new()), ty));
        }
        return Err(Error::at(
            span,
            format!(
                "type constructor `{}` expects {} parameter(s) in `[]`",
                name,
                info.param_tys.len()
            ),
        ));
    }
    if let Some(ty) = globals.get_ty(name) {
        let ty = eval(globals, &Vec::new(), ty);
        return Ok((const_(name), ty));
    }
    Err(Error::at(span, format!("unknown name `{}`", name)))
}

fn elab_enum_app(
    globals: &Globals,
    ctx: &mut Ctx,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Result<(RcTm, V)> {
    let info = globals.get_enum(name).unwrap();
    let param_tys = info.param_tys.clone();
    let sort = info.sort;
    if args.len() != param_tys.len() {
        return Err(Error::at(
            span,
            format!(
                "`{}` expects {} parameter(s) in `[]`, got {}",
                name,
                param_tys.len(),
                args.len()
            ),
        ));
    }
    let mut local_env: Env = Vec::new();
    let mut terms = Vec::new();
    for (arg, pty) in args.iter().zip(param_tys.iter()) {
        let dom = eval(globals, &local_env, pty);
        let tm = elab_check(globals, ctx, arg, &dom)?;
        let v = eval(globals, &ctx.env, &tm);
        local_env.push(v);
        terms.push(tm);
    }
    Ok((ind(name, terms), check::sort_value(sort)))
}

fn elab_app(
    globals: &Globals,
    ctx: &mut Ctx,
    f: &Expr,
    implicit: &[Expr],
    args: &[Expr],
    span: Span,
) -> Result<(RcTm, V)> {
    elab_app_with(globals, ctx, f, implicit, args, None, span)
}

/// Elaborate an application whose result is checked against `expected`.
/// The expected type participates in implicit-argument inference.
fn elab_app_check(
    globals: &Globals,
    ctx: &mut Ctx,
    f: &Expr,
    implicit: &[Expr],
    args: &[Expr],
    expected: &V,
    span: Span,
) -> Result<RcTm> {
    let (tm, ty) = elab_app_with(globals, ctx, f, implicit, args, Some(expected), span)?;
    if conv_ty(globals, ctx.level(), &ty, expected) {
        Ok(tm)
    } else {
        let found = crate::pretty::show_val(globals, ctx.level(), &ty);
        let want = crate::pretty::show_val(globals, ctx.level(), expected);
        Err(Error::at(
            span,
            format!("type mismatch: expected `{}`, found `{}`", want, found),
        ))
    }
}

fn elab_app_with(
    globals: &Globals,
    ctx: &mut Ctx,
    f: &Expr,
    implicit: &[Expr],
    args: &[Expr],
    expected: Option<&V>,
    span: Span,
) -> Result<(RcTm, V)> {
    if let Expr::Var(name, _) = f
        && ctx.lookup_index(name).is_none()
        && globals.get_enum(name).is_some()
    {
        if !args.is_empty() {
            return Err(Error::at(
                span,
                format!("the parameters of `{}` are written in `[]`, not `()`", name),
            ));
        }
        return elab_enum_app(globals, ctx, name, implicit, span);
    }

    if let Expr::Var(name, _) = f
        && ctx.lookup_index(name).is_none()
        && let Some(op) = PrimOp::from_name(name)
    {
        if !implicit.is_empty() {
            return Err(Error::at(
                span,
                format!("primitive `{}` takes no implicit arguments", op.name()),
            ));
        }
        if args.len() != 2 {
            return Err(Error::at(
                span,
                format!(
                    "primitive `{}` takes 2 arguments, got {}",
                    op.name(),
                    args.len()
                ),
            ));
        }
        let word = vprimty(PrimTy::U32);
        let a = elab_check(globals, ctx, &args[0], &word)?;
        let b = elab_check(globals, ctx, &args[1], &word)?;
        let ty = check::prim_result_ty(globals, op)?;
        return Ok((prim_op(op, a, b), ty));
    }

    let (mut f_tm, mut f_ty) = elab_infer(globals, ctx, f)?;

    // Apply any explicitly supplied implicit arguments.
    for arg in implicit {
        let ft = force(globals, f_ty);
        let (dom, closure) = match &*ft {
            Val::Pi(dom, closure, true) => (dom.clone(), closure.clone()),
            Val::Pi(_, _, false) => {
                return Err(Error::at(
                    arg.span(),
                    "too many implicit arguments for this function (it has no implicit parameter here)",
                ));
            }
            _ => {
                return Err(Error::at(
                    arg.span(),
                    "too many implicit arguments for this function",
                ));
            }
        };
        let arg_tm = elab_check(globals, ctx, arg, &dom)?;
        let arg_v = eval(globals, &ctx.env, &arg_tm);
        f_tm = app(f_tm, arg_tm);
        f_ty = eval_closure(globals, &closure, arg_v);
    }

    // The remaining leading implicit parameters are inferred.
    let k = leading_implicit_count(globals, ctx.level(), &f_ty);
    let call = ImplicitCall {
        f_tm: &f_tm,
        f_ty: &f_ty,
        args,
        expected,
        span,
    };
    match try_implicit_app(globals, ctx, &call, k) {
        ImplicitResult::Ok(tm, ty) => Ok((tm, ty)),
        ImplicitResult::Failed(err) => Err(err),
        ImplicitResult::NoParams => Err(Error::at(
            span,
            "application: the function has a non-function type",
        )),
    }
}

fn leading_implicit_count(globals: &Globals, base: usize, ty: &V) -> usize {
    let mut ty = force(globals, ty.clone());
    let mut count = 0;
    while let Val::Pi(_, closure, true) = &*ty {
        let closure = closure.clone();
        ty = eval_closure(globals, &closure, vvar(base + count));
        count += 1;
    }
    count
}

enum ImplicitResult {
    Ok(RcTm, V),
    NoParams,
    Failed(Error),
}

struct MetaInfo {
    level: usize,
    ty: V,
}

struct ImplicitCall<'a> {
    f_tm: &'a RcTm,
    f_ty: &'a V,
    args: &'a [Expr],
    expected: Option<&'a V>,
    span: Span,
}

fn meta_name(level: usize) -> String {
    // A control character cannot appear in a source identifier, so an
    // internal meta never shadows a user name.
    format!("\u{1}implicit{}", level)
}

/// Insert inferred arguments for the first `k` (implicit) parameters, then
/// elaborate the explicit arguments in the extended context.
fn try_implicit_app(
    globals: &Globals,
    ctx: &Ctx,
    call: &ImplicitCall<'_>,
    k: usize,
) -> ImplicitResult {
    let mut work = ctx.clone();
    let base = work.level();
    let mut ty = call.f_ty.clone();
    let mut metas = Vec::with_capacity(k);
    for _ in 0..k {
        let ft = force(globals, ty);
        let (dom, closure) = match &*ft {
            Val::Pi(dom, closure, _) => (dom.clone(), closure.clone()),
            _ => return ImplicitResult::NoParams,
        };
        let level = work.level();
        work.push(meta_name(level), dom.clone());
        metas.push(MetaInfo {
            level,
            ty: dom.clone(),
        });
        ty = eval_closure(globals, &closure, vvar(level));
    }
    let levels: Vec<usize> = metas.iter().map(|m| m.level).collect();
    let ext_level = work.level();

    let mut tm = shift(call.f_tm, k as isize, 0);
    for meta in &metas {
        tm = app(tm, var(ext_level - 1 - meta.level));
    }

    let mut sols = std::collections::HashMap::new();
    for arg in call.args {
        let ft = force(globals, ty);
        let (dom, closure) = match &*ft {
            Val::Pi(dom, closure, _) => (dom.clone(), closure.clone()),
            _ => {
                return ImplicitResult::Failed(Error::at(
                    call.span,
                    "application: the function has a non-function type",
                ));
            }
        };
        let dom = check::apply_solutions(globals, work.level(), &levels, &sols, &dom);
        let arg_tm = {
            let mut meta = MetaSolver {
                levels: &levels,
                sols: &mut sols,
            };
            match elab_arg_with_metas(globals, &mut work, arg, &dom, &mut meta) {
                Ok(tm) => tm,
                Err(err) => return ImplicitResult::Failed(err),
            }
        };
        let arg_v = eval(globals, &work.env, &arg_tm);
        tm = app(tm, arg_tm);
        ty = eval_closure(globals, &closure, arg_v);
    }

    let ty = check::apply_solutions(globals, work.level(), &levels, &sols, &ty);
    if let Some(exp) = call.expected
        && let Err(err) = unify_meta(globals, work.level(), &levels, &mut sols, &ty, exp)
    {
        return ImplicitResult::Failed(err);
    }
    if let Err(err) = resolve_metas(globals, work.level(), &metas, &mut sols) {
        return ImplicitResult::Failed(err);
    }

    let ty = check::apply_solutions(globals, work.level(), &levels, &sols, &ty);
    let tm = strip_metas(globals, base, &metas, &sols, tm);
    let ty_tm = quote(globals, ext_level, &ty);
    let ty_tm = strip_metas(globals, base, &metas, &sols, ty_tm);
    let ty = eval(globals, &ctx.env, &ty_tm);
    if let Some(exp) = call.expected
        && !conv_ty(globals, ctx.level(), &ty, exp)
    {
        let found = crate::pretty::show_val(globals, ctx.level(), &ty);
        let want = crate::pretty::show_val(globals, ctx.level(), exp);
        return ImplicitResult::Failed(Error::at(
            call.span,
            format!("type mismatch: expected `{}`, found `{}`", want, found),
        ));
    }
    ImplicitResult::Ok(tm, ty)
}

fn elab_arg_with_metas(
    globals: &Globals,
    ctx: &mut Ctx,
    arg: &Expr,
    dom: &V,
    meta: &mut MetaSolver<'_>,
) -> Result<RcTm> {
    if let Some((name, args, span)) = ctor_application(arg)
        && ctx.lookup_index(name).is_none()
        && globals.is_constructor(name)
    {
        return elab_ctor_check_metas(globals, ctx, name, args, dom, meta, span);
    }
    if !value_mentions_any(globals, ctx.level(), dom, meta.levels) {
        return elab_check(globals, ctx, arg, dom);
    }
    // An expected type that mentions a metavariable still carries useful
    // information for constructors and lambdas, so try checking first.
    let mut trial = ctx.clone();
    if let Ok(tm) = elab_check(globals, &mut trial, arg, dom) {
        return Ok(tm);
    }
    let (tm, ty) = elab_infer(globals, ctx, arg)?;
    unify_meta(globals, ctx.level(), meta.levels, meta.sols, &ty, dom)?;
    Ok(tm)
}

fn ctor_application(expr: &Expr) -> Option<(&str, &[Expr], Span)> {
    match expr {
        Expr::Var(name, span) => Some((name.as_str(), &[], *span)),
        Expr::App {
            f,
            implicit,
            args,
            span,
        } if implicit.is_empty() => {
            if let Expr::Var(name, _) = &**f {
                Some((name.as_str(), args, *span))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// First-order unification with the fresh implicit arguments as flexible
/// variables. It is deliberately conservative: higher-order meta applications
/// and escaping solutions are rejected.
fn unify_meta(
    globals: &Globals,
    level: usize,
    levels: &[usize],
    sols: &mut std::collections::HashMap<usize, V>,
    a: &V,
    b: &V,
) -> Result<()> {
    let a = force(
        globals,
        check::apply_solutions(globals, level, levels, sols, a),
    );
    let b = force(
        globals,
        check::apply_solutions(globals, level, levels, sols, b),
    );
    if conv(globals, level, &a, &b) {
        return Ok(());
    }
    if let Some(var) = as_meta(&a, levels) {
        return bind_meta(globals, level, levels, sols, var, &b);
    }
    if let Some(var) = as_meta(&b, levels) {
        return bind_meta(globals, level, levels, sols, var, &a);
    }

    match (&*a, &*b) {
        (Val::Pi(da, ca, _), Val::Pi(db, cb, _)) => {
            unify_meta(globals, level, levels, sols, da, db)?;
            let fresh = vvar(level);
            let ca = eval_closure(globals, ca, fresh.clone());
            let cb = eval_closure(globals, cb, fresh);
            unify_meta(globals, level + 1, levels, sols, &ca, &cb)
        }
        (Val::Ind(n1, a1), Val::Ind(n2, a2)) if n1 == n2 => {
            unify_meta_lists(globals, level, levels, sols, a1, a2)
        }
        (Val::Ctor(n1, i1, a1), Val::Ctor(n2, i2, a2)) if n1 == n2 && i1 == i2 => {
            unify_meta_lists(globals, level, levels, sols, a1, a2)
        }
        (Val::Eq(t1, l1, r1), Val::Eq(t2, l2, r2)) => {
            unify_meta(globals, level, levels, sols, t1, t2)?;
            unify_meta(globals, level, levels, sols, l1, l2)?;
            unify_meta(globals, level, levels, sols, r1, r2)
        }
        (Val::Refl(x), Val::Refl(y)) => unify_meta(globals, level, levels, sols, x, y),
        (Val::Neutral(n1), Val::Neutral(n2)) => match (&**n1, &**n2) {
            (Neutral::App(f1, a1), Neutral::App(f2, a2)) => {
                unify_meta(
                    globals,
                    level,
                    levels,
                    sols,
                    &vneutral((**f1).clone()),
                    &vneutral((**f2).clone()),
                )?;
                unify_meta(globals, level, levels, sols, a1, a2)
            }
            (Neutral::Global(x), Neutral::Global(y)) if x == y => Ok(()),
            _ => Err(unify_failure(globals, level, &a, &b)),
        },
        (Val::Type(u), Val::Type(v)) if u == v => Ok(()),
        (Val::Prop, Val::Prop) => Ok(()),
        _ => Err(unify_failure(globals, level, &a, &b)),
    }
}

fn unify_meta_lists(
    globals: &Globals,
    level: usize,
    levels: &[usize],
    sols: &mut std::collections::HashMap<usize, V>,
    a: &[V],
    b: &[V],
) -> Result<()> {
    if a.len() != b.len() {
        return Err(Error::new(
            "cannot infer implicit arguments: length mismatch",
        ));
    }
    for (x, y) in a.iter().zip(b.iter()) {
        unify_meta(globals, level, levels, sols, x, y)?;
    }
    Ok(())
}

fn as_meta(value: &V, levels: &[usize]) -> Option<usize> {
    if let Val::Neutral(n) = &**value
        && let Neutral::Var(level) = &**n
        && levels.contains(level)
    {
        Some(*level)
    } else {
        None
    }
}

fn bind_meta(
    globals: &Globals,
    level: usize,
    levels: &[usize],
    sols: &mut std::collections::HashMap<usize, V>,
    var: usize,
    value: &V,
) -> Result<()> {
    if let Some(existing) = sols.get(&var).cloned() {
        return unify_meta(globals, level, levels, sols, &existing, value);
    }
    let free = free_var_levels(globals, level, value);
    if free.contains(&var) {
        return Err(Error::new(
            "cannot infer implicit argument: cyclic index solution",
        ));
    }
    for other in &free {
        if *other >= var && !levels.contains(other) {
            return Err(Error::new(
                "cannot infer implicit argument: solution would escape its scope",
            ));
        }
    }
    sols.insert(var, value.clone());
    Ok(())
}

fn resolve_metas(
    globals: &Globals,
    level: usize,
    metas: &[MetaInfo],
    sols: &mut std::collections::HashMap<usize, V>,
) -> Result<()> {
    let levels: Vec<usize> = metas.iter().map(|m| m.level).collect();
    for _ in 0..=levels.len() {
        for meta in metas {
            if let Some(solution) = sols.get(&meta.level).cloned() {
                let solution = check::apply_solutions(globals, level, &levels, sols, &solution);
                sols.insert(meta.level, solution);
            }
        }
    }
    for meta in metas {
        let Some(solution) = sols.get(&meta.level) else {
            let ty = crate::pretty::show_val(globals, level, &meta.ty);
            return Err(Error::new(format!(
                "cannot infer implicit argument of type `{}`; pass it explicitly with `[...]`",
                ty
            )));
        };
        let free = free_var_levels(globals, level, solution);
        if free.iter().any(|l| levels.contains(l)) {
            return Err(Error::new(
                "cannot infer implicit arguments: cyclic solutions",
            ));
        }
    }
    Ok(())
}

/// Remove the fresh implicit-argument variables from a term, substituting their
/// solved values. The metas are the innermost locals, in order.
fn strip_metas(
    globals: &Globals,
    base: usize,
    metas: &[MetaInfo],
    sols: &std::collections::HashMap<usize, V>,
    mut tm: RcTm,
) -> RcTm {
    for (i, meta) in metas.iter().enumerate().rev() {
        let solution = sols
            .get(&meta.level)
            .expect("internal: unresolved implicit argument");
        let replacement = quote(globals, base + i, solution);
        tm = instantiate(&tm, &replacement);
    }
    tm
}

fn value_mentions_any(globals: &Globals, level: usize, value: &V, levels: &[usize]) -> bool {
    free_var_levels(globals, level, value)
        .iter()
        .any(|l| levels.contains(l))
}

fn free_var_levels(globals: &Globals, level: usize, value: &V) -> Vec<usize> {
    let tm = quote(globals, level, value);
    let mut out = Vec::new();
    collect_free_levels(&tm, level, 0, &mut out);
    out.sort_unstable();
    out.dedup();
    out
}

fn collect_free_levels(tm: &RcTm, level: usize, depth: usize, out: &mut Vec<usize>) {
    match &**tm {
        Tm::Var(i) if *i >= depth => {
            out.push(level + depth - 1 - *i);
        }
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop | Tm::PrimTy(_) | Tm::PrimLit(_) => {}
        Tm::PrimOp(_, a, b) => {
            collect_free_levels(a, level, depth, out);
            collect_free_levels(b, level, depth, out);
        }
        Tm::Pi { dom, cod, .. } | Tm::Lam { dom, body: cod, .. } => {
            collect_free_levels(dom, level, depth, out);
            collect_free_levels(cod, level, depth + 1, out);
        }
        Tm::App(f, a) => {
            collect_free_levels(f, level, depth, out);
            collect_free_levels(a, level, depth, out);
        }
        Tm::Ind(_, args) | Tm::Ctor(_, _, args) => {
            for arg in args {
                collect_free_levels(arg, level, depth, out);
            }
        }
        Tm::Match {
            motive,
            scrut,
            branches,
        } => {
            collect_free_levels(motive, level, depth, out);
            collect_free_levels(scrut, level, depth, out);
            for branch in branches {
                collect_free_levels(&branch.body, level, depth + branch.binders, out);
            }
        }
        Tm::Eq(a, b, c) => {
            collect_free_levels(a, level, depth, out);
            collect_free_levels(b, level, depth, out);
            collect_free_levels(c, level, depth, out);
        }
        Tm::Refl(a) => collect_free_levels(a, level, depth, out),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => {
            collect_free_levels(ty, level, depth, out);
            collect_free_levels(lhs, level, depth, out);
            collect_free_levels(rhs, level, depth, out);
            collect_free_levels(proof, level, depth, out);
            collect_free_levels(motive, level, depth, out);
            collect_free_levels(refl_case, level, depth, out);
        }
    }
}

fn unify_failure(globals: &Globals, level: usize, a: &V, b: &V) -> Error {
    let a = crate::pretty::show_val(globals, level, a);
    let b = crate::pretty::show_val(globals, level, b);
    Error::new(format!(
        "cannot infer implicit arguments: `{}` and `{}` are not equal",
        a, b
    ))
}

fn elab_pi(globals: &Globals, ctx: &mut Ctx, binders: &[Binder], body: &Expr) -> Result<(RcTm, V)> {
    let mut doms = Vec::new();
    for b in binders {
        let tm = elab_type(globals, ctx, &b.ty)?;
        let v = eval(globals, &ctx.env, &tm);
        let name = b
            .name
            .clone()
            .unwrap_or_else(|| format!("_x{}", doms.len()));
        ctx.push(name.clone(), v);
        doms.push((name, tm, b.implicit));
    }
    let cod = elab_type(globals, ctx, body)?;
    for _ in 0..doms.len() {
        ctx.pop();
    }
    let mut tm = cod;
    for (name, dom, implicit) in doms.into_iter().rev() {
        tm = pi_with(name, dom, tm, implicit);
    }
    let ty = check::infer(globals, ctx, &tm)?;
    Ok((tm, ty))
}

fn elab_lam_infer(
    globals: &Globals,
    ctx: &mut Ctx,
    binders: &[Binder],
    body: &Expr,
    span: Span,
) -> Result<(RcTm, V)> {
    let mut doms = Vec::new();
    for b in binders {
        let tm = elab_type(globals, ctx, &b.ty)?;
        let v = eval(globals, &ctx.env, &tm);
        let name = b
            .name
            .clone()
            .unwrap_or_else(|| format!("_x{}", doms.len()));
        ctx.push(name.clone(), v);
        doms.push((name, tm, b.implicit));
    }
    if doms.is_empty() {
        return Err(Error::at(span, "a lambda must bind at least one argument"));
    }
    let (body_tm, body_ty) = elab_infer(globals, ctx, body)?;
    let cod = quote(globals, ctx.level(), &body_ty);
    for _ in 0..doms.len() {
        ctx.pop();
    }
    let mut tm = body_tm;
    let mut ty_tm = cod;
    for (name, dom, implicit) in doms.into_iter().rev() {
        tm = lam(name.clone(), dom.clone(), tm);
        ty_tm = pi_with(name, dom, ty_tm, implicit);
    }
    let ty = eval(globals, &ctx.env, &ty_tm);
    Ok((tm, ty))
}

fn elab_eq(globals: &Globals, ctx: &mut Ctx, lhs: &Expr, rhs: &Expr) -> Result<(RcTm, V)> {
    let (lhs_tm, lhs_ty) = elab_infer(globals, ctx, lhs)?;
    let rhs_tm = elab_check(globals, ctx, rhs, &lhs_ty)?;
    let ty_tm = quote(globals, ctx.level(), &lhs_ty);
    Ok((eq(ty_tm, lhs_tm, rhs_tm), vprop()))
}

fn elab_block_infer(globals: &Globals, ctx: &mut Ctx, block: &Block) -> Result<(RcTm, V)> {
    let saved = elab_bindings(globals, ctx, block)?;
    let (tail_tm, tail_ty) = elab_infer(globals, ctx, &block.tail)?;
    // Compute the result type before popping the bindings. Kernel inference of
    // the let-chain would lose the implicit-parameter metadata of a function
    // value, so substitute through the binding types instead.
    let ty = type_after_bindings(globals, ctx, &saved, &tail_ty)?;
    for _ in &block.bindings {
        ctx.pop();
    }
    let mut tm = tail_tm;
    for (name, dom, value) in saved.into_iter().rev() {
        tm = app(lam(name, dom, tm), value);
    }
    Ok((tm, ty))
}

/// Result type of a nested chain of let-bindings `((fn(x: A) => tail) value)`.
///
/// `saved` is in binding order: each domain and value lives in the context
/// prefix that precedes its binding, and `tail_ty` lives in the fully extended
/// context. `ctx` must still contain every binding.
fn type_after_bindings(
    globals: &Globals,
    ctx: &Ctx,
    saved: &[(String, RcTm, RcTm)],
    tail_ty: &V,
) -> Result<V> {
    let base = ctx.level() - saved.len();
    let mut ty = tail_ty.clone();
    for i in (0..saved.len()).rev() {
        let (name, dom, value) = &saved[i];
        let cod = quote(globals, base + i + 1, &ty);
        let pi_tm = pi_with(name.clone(), dom.clone(), cod, false);
        let here = ctx.env[..base + i + 1].to_vec();
        let pi_v = eval(globals, &here, &pi_tm);
        let prefix = ctx.env[..base + i].to_vec();
        let value_v = eval(globals, &prefix, value);
        ty = apply_pi(globals, pi_v, value_v)
            .ok_or_else(|| Error::new("internal: binding type is not a function"))?;
    }
    Ok(ty)
}

fn elab_block_check(globals: &Globals, ctx: &mut Ctx, block: &Block, expected: &V) -> Result<RcTm> {
    let saved = elab_bindings(globals, ctx, block)?;
    let tail_tm = elab_check(globals, ctx, &block.tail, expected)?;
    for _ in &block.bindings {
        ctx.pop();
    }
    let mut tm = tail_tm;
    for (name, dom, value) in saved.into_iter().rev() {
        tm = app(lam(name, dom, tm), value);
    }
    Ok(tm)
}

fn elab_bindings(
    globals: &Globals,
    ctx: &mut Ctx,
    block: &Block,
) -> Result<Vec<(String, RcTm, RcTm)>> {
    let mut saved = Vec::new();
    for b in &block.bindings {
        let (value_tm, value_ty, dom) = match &b.ty {
            Some(ty_expr) => {
                let dom = elab_type(globals, ctx, ty_expr)?;
                let value_ty = eval(globals, &ctx.env, &dom);
                let value_tm = elab_check(globals, ctx, &b.value, &value_ty)?;
                (value_tm, value_ty, dom)
            }
            None => {
                let (value_tm, value_ty) = elab_infer(globals, ctx, &b.value)?;
                let dom = quote(globals, ctx.level(), &value_ty);
                (value_tm, value_ty, dom)
            }
        };
        saved.push((b.name.clone(), dom, value_tm));
        ctx.push(b.name.clone(), value_ty);
    }
    Ok(saved)
}

pub fn elab_check(globals: &Globals, ctx: &mut Ctx, expr: &Expr, expected: &V) -> Result<RcTm> {
    match expr {
        Expr::FnLam(binders, body, span) => {
            elab_lam_check(globals, ctx, binders, body, expected, *span)
        }
        Expr::Numeral(n, suffix, span) => {
            elab_numeral_check(globals, ctx, *n, *suffix, *span, expected)
        }
        Expr::Refl(span) => elab_refl(globals, ctx, expected, *span),
        Expr::Var(n, span) => {
            if ctx.lookup_index(n).is_none() && globals.is_constructor(n) {
                elab_ctor_check(globals, ctx, n, &[], expected, *span)
            } else {
                infer_and_check(globals, ctx, expr, expected)
            }
        }
        Expr::App {
            f,
            implicit,
            args,
            span,
        } => {
            if let Expr::Var(n, _) = &**f
                && ctx.lookup_index(n).is_none()
                && globals.is_constructor(n)
            {
                if !implicit.is_empty() {
                    return Err(Error::at(
                        *span,
                        "constructor type arguments are inferred from the expected type; remove the `[...]` arguments",
                    ));
                }
                return elab_ctor_check(globals, ctx, n, args, expected, *span);
            }
            elab_app_check(globals, ctx, f, implicit, args, expected, *span)
        }
        Expr::BinOp { op, lhs, rhs, span } => {
            elab_binop_check(globals, ctx, *op, lhs, rhs, *span, expected)
        }
        Expr::Match(scrut, arms, span) => {
            let (tm, _ty) = elab_match(globals, ctx, scrut, arms, Some(expected), *span)?;
            Ok(tm)
        }
        Expr::Ascribe(e, ty, _) => {
            let tm_ty = elab_type(globals, ctx, ty)?;
            let v = eval(globals, &ctx.env, &tm_ty);
            let tm = elab_check(globals, ctx, e, &v)?;
            if conv_ty(globals, ctx.level(), &v, expected) {
                Ok(tm)
            } else {
                let found = crate::pretty::show_val(globals, ctx.level(), &v);
                let want = crate::pretty::show_val(globals, ctx.level(), expected);
                Err(Error::at(
                    expr.span(),
                    format!("type mismatch: expected `{}`, found `{}`", want, found),
                ))
            }
        }
        Expr::Block(b) => elab_block_check(globals, ctx, b, expected),
        Expr::Paren(e, _) => elab_check(globals, ctx, e, expected),
        _ => infer_and_check(globals, ctx, expr, expected),
    }
}

fn infer_and_check(globals: &Globals, ctx: &mut Ctx, expr: &Expr, expected: &V) -> Result<RcTm> {
    let (tm, ty) = elab_infer(globals, ctx, expr)?;
    if conv_ty(globals, ctx.level(), &ty, expected) {
        return Ok(tm);
    }
    let k = leading_implicit_count(globals, ctx.level(), &ty);
    if k > 0 {
        let call = ImplicitCall {
            f_tm: &tm,
            f_ty: &ty,
            args: &[],
            expected: Some(expected),
            span: expr.span(),
        };
        match try_implicit_app(globals, ctx, &call, k) {
            ImplicitResult::Ok(tm, ty) if conv_ty(globals, ctx.level(), &ty, expected) => {
                return Ok(tm);
            }
            ImplicitResult::Failed(err) => return Err(err),
            _ => {}
        }
    }
    let found = crate::pretty::show_val(globals, ctx.level(), &ty);
    let want = crate::pretty::show_val(globals, ctx.level(), expected);
    Err(Error::at(
        expr.span(),
        format!("type mismatch: expected `{}`, found `{}`", want, found),
    ))
}

fn elab_ctor_fields(
    globals: &Globals,
    ctx: &mut Ctx,
    ci: &CtorInfo,
    param_vals: &[V],
    args: &[Expr],
    span: Span,
) -> Result<(Vec<RcTm>, V)> {
    if args.len() != ci.num_fields {
        return Err(Error::at(
            span,
            format!(
                "constructor `{}` takes {} field(s), got {}",
                ci.name,
                ci.num_fields,
                args.len()
            ),
        ));
    }
    let mut field_ty = eval(globals, &Vec::new(), &ci.ty);
    for p in param_vals {
        field_ty = apply_pi(globals, field_ty, p.clone())
            .ok_or_else(|| Error::new("constructor parameter mismatch"))?;
    }
    let mut field_tms = Vec::new();
    for arg in args {
        let ft = force(globals, field_ty);
        let (dom, closure) = match &*ft {
            Val::Pi(dom, closure, _) => (dom.clone(), closure.clone()),
            _ => return Err(Error::new("constructor field mismatch")),
        };
        let tm = elab_check(globals, ctx, arg, &dom)?;
        let v = eval(globals, &ctx.env, &tm);
        field_ty = eval_closure(globals, &closure, v);
        field_tms.push(tm);
    }
    Ok((field_tms, field_ty))
}

fn elab_ctor_check(
    globals: &Globals,
    ctx: &mut Ctx,
    name: &str,
    args: &[Expr],
    expected: &V,
    span: Span,
) -> Result<RcTm> {
    let ci = globals
        .get_ctor(name)
        .cloned()
        .ok_or_else(|| Error::at(span, format!("unknown constructor `{}`", name)))?;
    let expected_f = force(globals, expected.clone());
    let param_vals = match &*expected_f {
        Val::Ind(en, ps) if *en == ci.enum_name => ps.clone(),
        _ => {
            let want = crate::pretty::show_val(globals, ctx.level(), expected);
            return Err(Error::at(
                span,
                format!(
                    "cannot infer the type arguments of `{}` here; expected `{}`",
                    name, want
                ),
            ));
        }
    };
    let (field_tms, result) = elab_ctor_fields(globals, ctx, &ci, &param_vals, args, span)?;
    if !conv(globals, ctx.level(), &result, &expected_f) {
        return Err(Error::at(
            span,
            format!("constructor `{}` does not produce the expected type", name),
        ));
    }
    let mut all: Vec<RcTm> = param_vals
        .iter()
        .map(|p| quote(globals, ctx.level(), p))
        .collect();
    all.extend(field_tms);
    Ok(rc(Tm::Ctor(ci.enum_name.clone(), ci.index, all)))
}

struct MetaSolver<'a> {
    levels: &'a [usize],
    sols: &'a mut std::collections::HashMap<usize, V>,
}

fn elab_ctor_check_metas(
    globals: &Globals,
    ctx: &mut Ctx,
    name: &str,
    args: &[Expr],
    expected: &V,
    meta: &mut MetaSolver<'_>,
    span: Span,
) -> Result<RcTm> {
    let ci = globals
        .get_ctor(name)
        .cloned()
        .ok_or_else(|| Error::at(span, format!("unknown constructor `{}`", name)))?;
    let expected_f = force(globals, expected.clone());
    let param_vals = match &*expected_f {
        Val::Ind(en, ps) if *en == ci.enum_name => ps.clone(),
        _ => {
            let want = crate::pretty::show_val(globals, ctx.level(), expected);
            return Err(Error::at(
                span,
                format!(
                    "cannot infer the type arguments of `{}` here; expected `{}`",
                    name, want
                ),
            ));
        }
    };
    if args.len() != ci.num_fields {
        return Err(Error::at(
            span,
            format!(
                "constructor `{}` takes {} field(s), got {}",
                ci.name,
                ci.num_fields,
                args.len()
            ),
        ));
    }
    let mut field_ty = eval(globals, &Vec::new(), &ci.ty);
    for p in &param_vals {
        field_ty = apply_pi(globals, field_ty, p.clone())
            .ok_or_else(|| Error::new("constructor parameter mismatch"))?;
    }
    let mut field_tms = Vec::new();
    for arg in args {
        let ft = force(globals, field_ty);
        let (dom, closure) = match &*ft {
            Val::Pi(dom, closure, _) => (dom.clone(), closure.clone()),
            _ => return Err(Error::new("constructor field mismatch")),
        };
        let dom = check::apply_solutions(globals, ctx.level(), meta.levels, meta.sols, &dom);
        let tm = elab_arg_with_metas(globals, ctx, arg, &dom, meta)?;
        let v = eval(globals, &ctx.env, &tm);
        field_ty = eval_closure(globals, &closure, v);
        field_tms.push(tm);
    }
    let result = force(globals, field_ty);
    unify_meta(
        globals,
        ctx.level(),
        meta.levels,
        meta.sols,
        &result,
        &expected_f,
    )?;
    let mut all: Vec<RcTm> = param_vals
        .iter()
        .map(|p| quote(globals, ctx.level(), p))
        .collect();
    all.extend(field_tms);
    Ok(rc(Tm::Ctor(ci.enum_name.clone(), ci.index, all)))
}

fn elab_ctor_infer(
    globals: &Globals,
    ctx: &mut Ctx,
    name: &str,
    args: &[Expr],
    span: Span,
) -> Result<(RcTm, V)> {
    let ci = globals
        .get_ctor(name)
        .cloned()
        .ok_or_else(|| Error::at(span, format!("unknown constructor `{}`", name)))?;
    if ci.num_params != 0 {
        return Err(Error::at(
            span,
            format!(
                "cannot infer the type arguments of `{}`; use it where the expected type is known",
                name
            ),
        ));
    }
    let (field_tms, result) = elab_ctor_fields(globals, ctx, &ci, &[], args, span)?;
    Ok((
        rc(Tm::Ctor(ci.enum_name.clone(), ci.index, field_tms)),
        result,
    ))
}

fn elab_refl(globals: &Globals, ctx: &mut Ctx, expected: &V, span: Span) -> Result<RcTm> {
    let expected = force(globals, expected.clone());
    let (lhs, rhs) = match &*expected {
        Val::Eq(_, lhs, rhs) => (lhs.clone(), rhs.clone()),
        _ => return Err(Error::at(span, "`refl` can only prove an equality")),
    };
    if !conv(globals, ctx.level(), &lhs, &rhs) {
        let l = crate::pretty::show_val(globals, ctx.level(), &lhs);
        let r = crate::pretty::show_val(globals, ctx.level(), &rhs);
        return Err(Error::at(
            span,
            format!("`refl` cannot prove `{} == {}` (sides are not equal)", l, r),
        ));
    }
    Ok(rc(Tm::Refl(quote(globals, ctx.level(), &lhs))))
}

fn elab_lam_check(
    globals: &Globals,
    ctx: &mut Ctx,
    binders: &[Binder],
    body: &Expr,
    expected: &V,
    span: Span,
) -> Result<RcTm> {
    let mut pushed = 0;
    let mut doms = Vec::new();
    let mut expected = expected.clone();
    for b in binders {
        let exp = force(globals, expected);
        let (dom_v, closure) = match &*exp {
            Val::Pi(dom, closure, _) => (dom.clone(), closure.clone()),
            _ => {
                return Err(Error::at(
                    span,
                    "this lambda has more binders than its expected function type",
                ));
            }
        };
        let dom_tm = elab_type(globals, ctx, &b.ty)?;
        let annotated = eval(globals, &ctx.env, &dom_tm);
        if !conv(globals, ctx.level(), &annotated, &dom_v) {
            return Err(Error::at(
                b.span,
                "lambda binder type does not match the expected function type",
            ));
        }
        let name = b
            .name
            .clone()
            .unwrap_or_else(|| format!("_x{}", doms.len()));
        let fresh = ctx.push(name.clone(), dom_v);
        doms.push((name, dom_tm));
        expected = eval_closure(globals, &closure, fresh);
        pushed += 1;
    }
    let body_tm = elab_check(globals, ctx, body, &expected)?;
    for _ in 0..pushed {
        ctx.pop();
    }
    let mut tm = body_tm;
    for (name, dom) in doms.into_iter().rev() {
        tm = lam(name, dom, tm);
    }
    Ok(tm)
}

const MAX_NAT_LITERAL: u64 = 1_000_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NumTy {
    Nat,
    U32,
}

impl NumTy {
    fn val(self) -> V {
        match self {
            NumTy::Nat => vind("Nat", Vec::new()),
            NumTy::U32 => vprimty(PrimTy::U32),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EqTy {
    Nat,
    U32,
    Bool,
    String,
    Unit,
}

fn vbool() -> V {
    vind("Bool", Vec::new())
}

fn classify_num_ty(globals: &Globals, ty: &V) -> Option<NumTy> {
    let ty = force(globals, ty.clone());
    match &*ty {
        Val::Ind(name, args) if name == "Nat" && args.is_empty() => Some(NumTy::Nat),
        Val::PrimTy(PrimTy::U32) => Some(NumTy::U32),
        _ => None,
    }
}

fn classify_eq_ty(globals: &Globals, ty: &V) -> Option<EqTy> {
    let ty = force(globals, ty.clone());
    match &*ty {
        Val::Ind(name, args) if args.is_empty() => match name.as_str() {
            "Nat" => Some(EqTy::Nat),
            "Bool" => Some(EqTy::Bool),
            "String" => Some(EqTy::String),
            "Unit" => Some(EqTy::Unit),
            _ => None,
        },
        Val::PrimTy(PrimTy::U32) => Some(EqTy::U32),
        _ => None,
    }
}

fn is_ambiguous_numeral(expr: &Expr) -> bool {
    match expr {
        Expr::Numeral(_, None, _) => true,
        Expr::Paren(e, _) => is_ambiguous_numeral(e),
        _ => false,
    }
}

fn elab_numeral_infer(n: u64, suffix: Option<NumSuffix>, span: Span) -> Result<(RcTm, V)> {
    match suffix {
        Some(NumSuffix::U32) => {
            let word = u32::try_from(n).map_err(|_| Error::at(span, "u32 literal is too large"))?;
            Ok((prim_lit(word), vprimty(PrimTy::U32)))
        }
        None => Err(Error::at(
            span,
            "cannot infer the type of this numeral; add an annotation, an ascription, or the `u32` suffix",
        )),
    }
}

fn elab_numeral_check(
    globals: &Globals,
    ctx: &Ctx,
    n: u64,
    suffix: Option<NumSuffix>,
    span: Span,
    expected: &V,
) -> Result<RcTm> {
    if let Some(NumSuffix::U32) = suffix {
        let word = u32::try_from(n).map_err(|_| Error::at(span, "u32 literal is too large"))?;
        let tm = prim_lit(word);
        if conv_ty(globals, ctx.level(), &vprimty(PrimTy::U32), expected) {
            return Ok(tm);
        }
        let want = crate::pretty::show_val(globals, ctx.level(), expected);
        return Err(Error::at(
            span,
            format!(
                "literal suffix `u32` conflicts with expected type `{}`",
                want
            ),
        ));
    }
    let expected = force(globals, expected.clone());
    match &*expected {
        Val::Ind(name, args) if name == "Nat" && args.is_empty() => nat_literal(globals, n, span),
        Val::PrimTy(PrimTy::U32) => {
            let word =
                u32::try_from(n).map_err(|_| Error::at(span, "literal does not fit in `U32`"))?;
            Ok(prim_lit(word))
        }
        Val::Neutral(_) => Err(Error::at(
            span,
            "cannot infer the type of this numeral; add an annotation, an ascription, or the `u32` suffix",
        )),
        _ => {
            let want = crate::pretty::show_val(globals, ctx.level(), &expected);
            Err(Error::at(
                span,
                format!("numeric literal cannot have type `{}`", want),
            ))
        }
    }
}

fn num_ty_or_error(globals: &Globals, ctx: &Ctx, span: Span, op: BinOp, ty: &V) -> Result<NumTy> {
    classify_num_ty(globals, ty).ok_or_else(|| {
        let found = crate::pretty::show_val(globals, ctx.level(), ty);
        Error::at(
            span,
            format!(
                "operator `{}` requires `Nat` or `U32`, found `{}`",
                op.symbol(),
                found
            ),
        )
    })
}

fn elab_numeric_operands(
    globals: &Globals,
    ctx: &mut Ctx,
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
    span: Span,
    hint: Option<&V>,
) -> Result<(RcTm, RcTm, NumTy)> {
    if let Some(ty) = hint
        && let Some(num) = classify_num_ty(globals, ty)
    {
        let a = elab_check(globals, ctx, lhs, ty)?;
        let b = elab_check(globals, ctx, rhs, ty)?;
        return Ok((a, b, num));
    }
    if !is_ambiguous_numeral(lhs) {
        let (a, ty) = elab_infer(globals, ctx, lhs)?;
        let num = num_ty_or_error(globals, ctx, span, op, &ty)?;
        let b = elab_check(globals, ctx, rhs, &ty)?;
        return Ok((a, b, num));
    }
    if !is_ambiguous_numeral(rhs) {
        let (b, ty) = elab_infer(globals, ctx, rhs)?;
        let num = num_ty_or_error(globals, ctx, span, op, &ty)?;
        let a = elab_check(globals, ctx, lhs, &ty)?;
        return Ok((a, b, num));
    }
    Err(Error::at(
        span,
        format!(
            "cannot infer the numeric type of `{}`; annotate an operand",
            op.symbol()
        ),
    ))
}

fn prim_op_of(op: BinOp) -> PrimOp {
    match op {
        BinOp::Add => PrimOp::Add,
        BinOp::Sub => PrimOp::Sub,
        BinOp::Mul => PrimOp::Mul,
        BinOp::Div => PrimOp::Div,
        BinOp::Rem => PrimOp::Rem,
        BinOp::And => PrimOp::And,
        BinOp::Or => PrimOp::Or,
        BinOp::Xor => PrimOp::Xor,
        BinOp::Shl => PrimOp::Shl,
        BinOp::Shr => PrimOp::Shr,
        BinOp::Lt => PrimOp::Lt,
        BinOp::Le => PrimOp::Le,
        BinOp::Gt => PrimOp::Gt,
        BinOp::Ge => PrimOp::Ge,
        BinOp::Eq => PrimOp::Eq,
        BinOp::Ne => PrimOp::Ne,
    }
}

fn nat_op_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "nat_add",
        BinOp::Sub => "nat_sub",
        BinOp::Mul => "nat_mul",
        BinOp::Div => "nat_div",
        BinOp::Rem => "nat_rem",
        BinOp::And => "nat_and",
        BinOp::Or => "nat_or",
        BinOp::Xor => "nat_xor",
        BinOp::Shl => "nat_shl",
        BinOp::Shr => "nat_shr",
        BinOp::Lt => "nat_lt",
        BinOp::Le => "nat_le",
        BinOp::Gt => "nat_gt",
        BinOp::Ge => "nat_ge",
        BinOp::Eq => "nat_eq",
        BinOp::Ne => "nat_ne",
    }
}

fn prelude_call(globals: &Globals, name: &str, a: RcTm, b: RcTm, span: Span) -> Result<RcTm> {
    if globals.get_ty(name).is_none() {
        return Err(Error::at(
            span,
            format!("internal: missing prelude function `{}`", name),
        ));
    }
    Ok(app(app(const_(name), a), b))
}

fn numeric_call(
    globals: &Globals,
    op: BinOp,
    a: RcTm,
    b: RcTm,
    ty: NumTy,
    span: Span,
) -> Result<RcTm> {
    match ty {
        NumTy::U32 => Ok(prim_op(prim_op_of(op), a, b)),
        NumTy::Nat => prelude_call(globals, nat_op_name(op), a, b, span),
    }
}

fn eq_ty_or_error(globals: &Globals, ctx: &Ctx, span: Span, ty: &V) -> Result<EqTy> {
    classify_eq_ty(globals, ty).ok_or_else(|| {
        let found = crate::pretty::show_val(globals, ctx.level(), ty);
        Error::at(
            span,
            format!("boolean equality is not defined for type `{}`", found),
        )
    })
}

fn equality_call(
    globals: &Globals,
    op: BinOp,
    ty: EqTy,
    a: RcTm,
    b: RcTm,
    span: Span,
) -> Result<RcTm> {
    let negated = op == BinOp::Ne;
    let name = match (ty, negated) {
        (EqTy::Nat, false) => "nat_eq",
        (EqTy::Nat, true) => "nat_ne",
        (EqTy::Bool, false) => "bool_eq",
        (EqTy::Bool, true) => "bool_ne",
        (EqTy::String, false) => "str_eq",
        (EqTy::String, true) => "str_ne",
        (EqTy::Unit, false) => "unit_eq",
        (EqTy::Unit, true) => "unit_ne",
        (EqTy::U32, false) => return Ok(prim_op(PrimOp::Eq, a, b)),
        (EqTy::U32, true) => return Ok(prim_op(PrimOp::Ne, a, b)),
    };
    prelude_call(globals, name, a, b, span)
}

fn elab_equality_op(
    globals: &Globals,
    ctx: &mut Ctx,
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
    span: Span,
) -> Result<RcTm> {
    let (a, b, ty) = if !is_ambiguous_numeral(lhs) {
        let (a, ty) = elab_infer(globals, ctx, lhs)?;
        let b = elab_check(globals, ctx, rhs, &ty)?;
        (a, b, ty)
    } else if !is_ambiguous_numeral(rhs) {
        let (b, ty) = elab_infer(globals, ctx, rhs)?;
        let a = elab_check(globals, ctx, lhs, &ty)?;
        (a, b, ty)
    } else {
        return Err(Error::at(
            span,
            "cannot infer the operand type of `==` or `!=`; annotate an operand",
        ));
    };
    let eq_ty = eq_ty_or_error(globals, ctx, span, &ty)?;
    equality_call(globals, op, eq_ty, a, b, span)
}

fn elab_binop_infer(
    globals: &Globals,
    ctx: &mut Ctx,
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
    span: Span,
) -> Result<(RcTm, V)> {
    if op.is_equality() {
        let tm = elab_equality_op(globals, ctx, op, lhs, rhs, span)?;
        return Ok((tm, vbool()));
    }
    let (a, b, num) = elab_numeric_operands(globals, ctx, op, lhs, rhs, span, None)?;
    let tm = numeric_call(globals, op, a, b, num, span)?;
    let ty = if op.is_comparison() {
        vbool()
    } else {
        num.val()
    };
    Ok((tm, ty))
}

fn elab_binop_check(
    globals: &Globals,
    ctx: &mut Ctx,
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
    span: Span,
    expected: &V,
) -> Result<RcTm> {
    if op.is_equality() {
        let tm = elab_equality_op(globals, ctx, op, lhs, rhs, span)?;
        return expect_ty(globals, ctx, span, tm, vbool(), expected);
    }
    let hint = if op.is_comparison() {
        None
    } else {
        Some(expected)
    };
    let (a, b, num) = elab_numeric_operands(globals, ctx, op, lhs, rhs, span, hint)?;
    let tm = numeric_call(globals, op, a, b, num, span)?;
    let ty = if op.is_comparison() {
        vbool()
    } else {
        num.val()
    };
    expect_ty(globals, ctx, span, tm, ty, expected)
}

fn expect_ty(
    globals: &Globals,
    ctx: &Ctx,
    span: Span,
    tm: RcTm,
    found: V,
    expected: &V,
) -> Result<RcTm> {
    if conv_ty(globals, ctx.level(), &found, expected) {
        return Ok(tm);
    }
    let found = crate::pretty::show_val(globals, ctx.level(), &found);
    let want = crate::pretty::show_val(globals, ctx.level(), expected);
    Err(Error::at(
        span,
        format!("type mismatch: expected `{}`, found `{}`", want, found),
    ))
}

fn nat_literal(globals: &Globals, n: u64, span: Span) -> Result<RcTm> {
    if n > MAX_NAT_LITERAL {
        return Err(Error::at(
            span,
            format!(
                "`Nat` literal is too large (limit {}); use a `u32` literal",
                MAX_NAT_LITERAL
            ),
        ));
    }
    let info = globals
        .get_enum("Nat")
        .ok_or_else(|| Error::at(span, "numeric literals require the `Nat` type"))?;
    let zero = info
        .ctors
        .iter()
        .position(|c| c.name == "zero")
        .ok_or_else(|| Error::at(span, "`Nat` must have a `zero` constructor"))?;
    let succ = info
        .ctors
        .iter()
        .position(|c| c.name == "succ")
        .ok_or_else(|| Error::at(span, "`Nat` must have a `succ` constructor"))?;
    let mut tm = rc(Tm::Ctor("Nat".to_string(), zero, Vec::new()));
    for _ in 0..n {
        tm = rc(Tm::Ctor("Nat".to_string(), succ, vec![tm]));
    }
    Ok(tm)
}

fn string_literal(globals: &Globals, text: &str, span: Span) -> Result<(RcTm, V)> {
    let info = globals
        .get_enum("String")
        .ok_or_else(|| Error::at(span, "string literals require the `String` type"))?;
    let nil = info
        .ctors
        .iter()
        .position(|c| c.name == "str_nil")
        .ok_or_else(|| Error::at(span, "`String` must have a `str_nil` constructor"))?;
    let cons = info
        .ctors
        .iter()
        .position(|c| c.name == "str_cons")
        .ok_or_else(|| Error::at(span, "`String` must have a `str_cons` constructor"))?;
    let mut tm = rc(Tm::Ctor("String".to_string(), nil, Vec::new()));
    for byte in text.as_bytes().iter().rev() {
        let head = nat_literal(globals, u64::from(*byte), span)?;
        tm = rc(Tm::Ctor("String".to_string(), cons, vec![head, tm]));
    }
    Ok((tm, vind("String", Vec::new())))
}

fn elab_match(
    globals: &Globals,
    ctx: &mut Ctx,
    scrut: &Expr,
    arms: &[Arm],
    expected: Option<&V>,
    span: Span,
) -> Result<(RcTm, V)> {
    let (scrut_tm, scrut_ty) = elab_infer(globals, ctx, scrut)?;
    let scrut_ty = force(globals, scrut_ty);

    if let Val::Eq(eq_ty, eq_lhs, eq_rhs) = &*scrut_ty {
        return elab_eq_match(
            globals,
            ctx,
            EqMatchData {
                proof: scrut_tm,
                ty: eq_ty.clone(),
                lhs: eq_lhs.clone(),
                rhs: eq_rhs.clone(),
            },
            arms,
            expected,
            span,
        );
    }

    let (enum_name, params) = match &*scrut_ty {
        Val::Ind(name, args) => (name.clone(), args.clone()),
        _ => {
            return Err(Error::at(
                span,
                "match: the scrutinee is not an enum value or an equality proof",
            ));
        }
    };
    let info = match globals.get_enum(&enum_name) {
        Some(i) => i.clone(),
        None => return Err(Error::at(span, format!("unknown enum `{}`", enum_name))),
    };

    // Refine each constructor's result indices against the scrutinee type.
    // This decides which constructors can produce a value of this type and
    // which field indices are forced by the scrutinee's indices.
    let mut refinements = Vec::with_capacity(info.ctors.len());
    for ctor in &info.ctors {
        refinements.push(check::refine_ctor(globals, ctx, ctor, &params, &scrut_ty)?);
    }

    // A bare identifier naming a nullary constructor is a constructor pattern,
    // not a binder.
    let arms: Vec<Arm> = arms
        .iter()
        .map(|arm| {
            let mut arm = arm.clone();
            if let Pattern::Var(n, s) = &arm.pattern
                && info.ctors.iter().any(|c| c.name == *n && c.num_fields == 0)
            {
                arm.pattern = Pattern::Ctor(n.clone(), Vec::new(), *s);
            }
            arm
        })
        .collect();
    let arms = &arms[..];

    // A single variable/wildcard arm is an irrefutable binding.
    if arms.len() == 1 && matches!(arms[0].pattern, Pattern::Var(..) | Pattern::Wild(..)) {
        let name = match &arms[0].pattern {
            Pattern::Var(n, _) => n.clone(),
            _ => "_".to_string(),
        };
        let dom = quote(globals, ctx.level(), &scrut_ty);
        ctx.push(name.clone(), scrut_ty.clone());
        let (body_tm, body_ty) = match expected {
            Some(exp) => (elab_check(globals, ctx, &arms[0].body, exp)?, exp.clone()),
            None => elab_infer(globals, ctx, &arms[0].body)?,
        };
        let ty = match expected {
            Some(exp) => exp.clone(),
            None => {
                let saved = [(name.clone(), dom.clone(), scrut_tm.clone())];
                type_after_bindings(globals, ctx, &saved, &body_ty)?
            }
        };
        ctx.pop();
        let tm = app(lam(name, dom, body_tm), scrut_tm);
        return Ok((tm, ty));
    }

    // Group arms by constructor, preserving source order inside each group.
    // Multiple arms for one constructor are allowed; the matrix compiler
    // below decides which of them applies.
    let mut groups: Vec<Vec<&Arm>> = vec![Vec::new(); info.ctors.len()];
    for arm in arms {
        match &arm.pattern {
            Pattern::Ctor(cname, subpats, aspan) => {
                let idx = match info.ctors.iter().position(|c| c.name == *cname) {
                    Some(i) => i,
                    None => {
                        return Err(Error::at(
                            *aspan,
                            format!("`{}` is not a constructor of `{}`", cname, enum_name),
                        ));
                    }
                };
                if subpats.len() != info.ctors[idx].num_fields {
                    return Err(Error::at(
                        *aspan,
                        format!(
                            "constructor `{}` has {} field(s), but the pattern binds {}",
                            cname,
                            info.ctors[idx].num_fields,
                            subpats.len()
                        ),
                    ));
                }
                groups[idx].push(arm);
            }
            _ => {
                return Err(Error::at(
                    arm.span,
                    "match arms must use constructor patterns (or a single variable pattern)",
                ));
            }
        }
    }
    // Exhaustiveness: every reachable constructor needs an arm. Arms for
    // impossible constructors are accepted but their bodies are not checked,
    // because no value of this scrutinee type can use them.
    for (i, group) in groups.iter().enumerate() {
        if group.is_empty() && refinements[i].possible {
            return Err(Error::at(
                span,
                format!(
                    "match is not exhaustive: missing constructor `{}`",
                    info.ctors[i].name
                ),
            ));
        }
    }

    let expected = expected.cloned();
    let mut bodies: Vec<Option<RcTm>> = vec![None; info.ctors.len()];
    let (motive, motive_v) = match &expected {
        Some(exp) => {
            let motive = build_motive(globals, ctx, &scrut_tm, &scrut_ty, exp);
            let motive_v = eval(globals, &ctx.env, &motive);
            for (idx, ctor) in info.ctors.iter().enumerate() {
                if !refinements[idx].possible {
                    continue;
                }
                let target = CtorTarget {
                    enum_name: &enum_name,
                    params: &params,
                    idx,
                    ctor,
                    refinement: &refinements[idx],
                };
                let (body, _) = compile_ctor_group(
                    globals,
                    ctx,
                    &target,
                    &groups[idx],
                    Some(&motive_v),
                    Some(exp),
                    span,
                )?;
                bodies[idx] = Some(body);
            }
            (motive, motive_v)
        }
        None => {
            let mut inferred: Option<V> = None;
            for (idx, ctor) in info.ctors.iter().enumerate() {
                if !refinements[idx].possible {
                    continue;
                }
                let target = CtorTarget {
                    enum_name: &enum_name,
                    params: &params,
                    idx,
                    ctor,
                    refinement: &refinements[idx],
                };
                let (body, body_ty) =
                    compile_ctor_group(globals, ctx, &target, &groups[idx], None, None, span)?;
                let body_ty =
                    body_ty.ok_or_else(|| Error::new("internal: missing inferred branch type"))?;
                match &inferred {
                    None => inferred = Some(body_ty),
                    Some(t) => {
                        if !conv_ty(globals, ctx.level(), t, &body_ty) {
                            return Err(Error::at(
                                span,
                                "match arms have different result types; add an expected type",
                            ));
                        }
                    }
                }
                bodies[idx] = Some(body);
            }
            let rt = inferred.ok_or_else(|| {
                Error::at(
                    span,
                    "cannot infer the result type of a match with no reachable branches; add an expected type",
                )
            })?;
            let dom = quote(globals, ctx.level(), &scrut_ty);
            let body = quote(globals, ctx.level() + 1, &rt);
            let motive = lam("_", dom, body);
            let motive_v = eval(globals, &ctx.env, &motive);
            (motive, motive_v)
        }
    };

    let mut branches = Vec::with_capacity(info.ctors.len());
    for (idx, ctor) in info.ctors.iter().enumerate() {
        if !refinements[idx].possible {
            // The constructor cannot produce a value of this scrutinee type.
            // Keep a placeholder so branch order still matches constructor
            // order; the kernel skips it and code generation omits it.
            branches.push(Branch {
                ctor: ctor.name.clone(),
                binders: ctor.num_fields,
                body: rc(Tm::Prop),
                impossible: true,
            });
            continue;
        }
        let body = bodies[idx]
            .take()
            .ok_or_else(|| Error::new("internal: reachable constructor has no body"))?;
        branches.push(Branch {
            ctor: ctor.name.clone(),
            binders: ctor.num_fields,
            body,
            impossible: false,
        });
    }

    let scrut_v = eval(globals, &ctx.env, &scrut_tm);
    let tm = rc(Tm::Match {
        motive,
        scrut: scrut_tm,
        branches,
    });
    let ty = vapp(globals, motive_v, scrut_v);
    Ok((tm, ty))
}

/// A branch-local context together with the index solutions that the
/// constructor refinement determined. The solutions live in the extended
/// context (`ctx` after the constructor fields have been pushed).
struct BranchCtx {
    ctx: Ctx,
    vars: Vec<usize>,
    solutions: std::collections::HashMap<usize, V>,
}

/// Extend the outer context with the constructor's fields and apply the index
/// solutions to every local type and to the branch environment.
///
/// The branch body keeps the same de Bruijn layout; only the types of the
/// locals and their evaluation environment are refined, so a variable may be
/// known to equal a constructor field without the term being rewritten.
fn refine_branch(
    globals: &Globals,
    ctx: &Ctx,
    refinement: &check::CtorRefinement,
    names: &[String],
) -> Result<BranchCtx> {
    if names.len() != refinement.field_tys.len() {
        return Err(Error::new("internal: constructor field mismatch"));
    }
    let mut branch_ctx = ctx.clone();
    for (name, ty) in names.iter().zip(refinement.field_tys.iter()) {
        branch_ctx.push(name.clone(), ty.clone());
    }
    let inner = ctx.level();
    let level = branch_ctx.level();

    let mut vars = Vec::new();
    let mut solutions = std::collections::HashMap::new();
    for (var, value) in &refinement.context_solutions {
        vars.push(*var);
        solutions.insert(*var, value.clone());
    }
    for (j, solution) in refinement.field_solutions.iter().enumerate() {
        if let Some(value) = solution {
            let var = inner + j;
            vars.push(var);
            solutions.insert(var, value.clone());
        }
    }

    for (_, ty) in branch_ctx.locals.iter_mut() {
        *ty = check::apply_solutions(globals, level, &vars, &solutions, ty);
    }
    for (var, value) in &solutions {
        branch_ctx.env[*var] = value.clone();
    }

    Ok(BranchCtx {
        ctx: branch_ctx,
        vars,
        solutions,
    })
}

/// One column of a pattern matrix. The value is a known term in the current
/// context: usually a local variable, but an index solution may make it a
/// constructor application.
#[derive(Clone)]
struct MatchColumn {
    ty: V,
    value: V,
}

/// One remaining arm of a pattern matrix. `bindings` records variables that
/// were bound by irrefutable patterns in earlier columns; they are substituted
/// into the body once the corresponding row is selected.
#[derive(Clone)]
struct MatchRow<'a> {
    pats: Vec<Pattern>,
    arm: &'a Arm,
    bindings: Vec<(String, V, V)>,
}

/// A constructor being elaborated, together with the enum parameters and the
/// index refinement that selected it.
struct CtorTarget<'a> {
    enum_name: &'a str,
    params: &'a [V],
    idx: usize,
    ctor: &'a CtorInfo,
    refinement: &'a check::CtorRefinement,
}

/// Compile the arms for one constructor of the outer match. All arms in the
/// group have the same outer constructor; their subpatterns form a matrix over
/// that constructor's fields.
fn compile_ctor_group(
    globals: &Globals,
    ctx: &Ctx,
    target: &CtorTarget<'_>,
    arms: &[&Arm],
    motive_v: Option<&V>,
    expected: Option<&V>,
    span: Span,
) -> Result<(RcTm, Option<V>)> {
    let ctor = target.ctor;
    let refinement = target.refinement;
    let names: Vec<String> = (0..ctor.num_fields).map(|j| format!("_f{}", j)).collect();
    let mut branch = refine_branch(globals, ctx, refinement, &names)?;
    let inner = ctx.level();
    let columns: Vec<MatchColumn> = (0..ctor.num_fields)
        .map(|j| {
            let value = match &refinement.field_solutions[j] {
                Some(solution) => solution.clone(),
                None => vvar(inner + j),
            };
            MatchColumn {
                ty: branch.ctx.locals[inner + j].1.clone(),
                value,
            }
        })
        .collect();
    let mut rows = Vec::with_capacity(arms.len());
    for arm in arms {
        let Pattern::Ctor(_, subpats, _) = &arm.pattern else {
            return Err(Error::new("internal: expected a constructor pattern"));
        };
        rows.push(MatchRow {
            pats: subpats.clone(),
            arm,
            bindings: Vec::new(),
        });
    }

    let expected_ty = match (motive_v, expected) {
        (Some(motive), Some(_)) => {
            let mut field_vals = Vec::with_capacity(ctor.num_fields);
            for (j, solution) in refinement.field_solutions.iter().enumerate() {
                match solution {
                    Some(solution) => field_vals.push(solution.clone()),
                    None => field_vals.push(vvar(inner + j)),
                }
            }
            let mut args = target.params.to_vec();
            args.extend(field_vals);
            let ctor_v = vctor(target.enum_name.to_string(), target.idx, args);
            let ty = vapp(globals, motive.clone(), ctor_v);
            Some(check::apply_solutions(
                globals,
                branch.ctx.level(),
                &branch.vars,
                &branch.solutions,
                &ty,
            ))
        }
        (None, None) => None,
        _ => return Err(Error::new("internal: inconsistent match expectation")),
    };

    let (body, body_ty) = elab_matrix(
        globals,
        &mut branch.ctx,
        &columns,
        &rows,
        expected_ty.as_ref(),
        span,
    )?;
    let level = branch.ctx.level();
    let body = substitute_field_solutions(
        globals,
        level,
        ctor.num_fields,
        &refinement.field_solutions,
        body,
    );
    Ok((
        body,
        if expected.is_none() {
            Some(body_ty)
        } else {
            None
        },
    ))
}

/// Compile a pattern matrix into nested core matches.
///
/// A column is a known value: usually a local variable, but index refinement
/// can make it a constructor application. Matching a constructor column refines
/// the context exactly like the outer match, then recursively compiles the
/// field matrix in the refined branch. A column whose patterns are all
/// variables or wildcards is dropped and its bindings are remembered for the
/// selected row.
fn elab_matrix<'a>(
    globals: &Globals,
    ctx: &mut Ctx,
    columns: &[MatchColumn],
    rows: &[MatchRow<'a>],
    expected: Option<&V>,
    span: Span,
) -> Result<(RcTm, V)> {
    if columns.is_empty() {
        let row = rows.first().ok_or_else(|| {
            Error::at(
                span,
                "match is not exhaustive: some nested value has no matching pattern",
            )
        })?;
        return elab_row_body(globals, ctx, row, expected);
    }

    let column = &columns[0];
    let col_ty = force(globals, column.ty.clone());
    let mut rows: Vec<MatchRow<'a>> = rows.to_vec();
    normalize_pattern_column(globals, &col_ty, &mut rows)?;

    if rows
        .iter()
        .all(|row| matches!(row.pats[0], Pattern::Var(..) | Pattern::Wild(..)))
    {
        let value = column.value.clone();
        let mut dropped = Vec::with_capacity(rows.len());
        for mut row in rows {
            if let Pattern::Var(name, _) = &row.pats[0] {
                row.bindings
                    .push((name.clone(), column.ty.clone(), value.clone()));
            }
            row.pats.remove(0);
            dropped.push(row);
        }
        return elab_matrix(globals, ctx, &columns[1..], &dropped, expected, span);
    }

    // An index solution can make a field's value a known constructor. In that
    // case there is nothing to match: decompose the value directly and check
    // each pattern against it.
    if let Val::Ctor(_, ctor_index, args) = &*force(globals, column.value.clone()) {
        let (enum_name, params) = match &*col_ty {
            Val::Ind(name, args) => (name.clone(), args.clone()),
            _ => {
                return Err(Error::at(
                    span,
                    "match: a constructor pattern requires an enum value",
                ));
            }
        };
        let info = globals
            .get_enum(&enum_name)
            .ok_or_else(|| Error::new(format!("unknown enum `{}`", enum_name)))?
            .clone();
        let ctor = info
            .ctors
            .get(*ctor_index)
            .ok_or_else(|| Error::new("internal: invalid constructor index"))?;
        if args.len() != ctor.num_params + ctor.num_fields {
            return Err(Error::new("internal: constructor argument mismatch"));
        }
        let fields = &args[ctor.num_params..];
        let field_tys = ctor_field_types(globals, ctor, &params, fields)?;
        let mut new_columns = Vec::with_capacity(ctor.num_fields + columns.len() - 1);
        for (ty, value) in field_tys.into_iter().zip(fields.iter().cloned()) {
            new_columns.push(MatchColumn { ty, value });
        }
        new_columns.extend(columns[1..].iter().cloned());

        let mut new_rows = Vec::new();
        for row in rows {
            match &row.pats[0] {
                Pattern::Ctor(name, subpats, _) if name == &ctor.name => {
                    let mut pats = subpats.clone();
                    pats.extend_from_slice(&row.pats[1..]);
                    new_rows.push(MatchRow {
                        pats,
                        arm: row.arm,
                        bindings: row.bindings,
                    });
                }
                Pattern::Ctor(..) => {}
                Pattern::Var(name, _) => {
                    let mut pats = vec![Pattern::Wild(row.pats[0].span()); ctor.num_fields];
                    pats.extend_from_slice(&row.pats[1..]);
                    let mut bindings = row.bindings;
                    bindings.push((name.clone(), column.ty.clone(), column.value.clone()));
                    new_rows.push(MatchRow {
                        pats,
                        arm: row.arm,
                        bindings,
                    });
                }
                Pattern::Wild(_) => {
                    let mut pats = vec![Pattern::Wild(row.pats[0].span()); ctor.num_fields];
                    pats.extend_from_slice(&row.pats[1..]);
                    new_rows.push(MatchRow {
                        pats,
                        arm: row.arm,
                        bindings: row.bindings,
                    });
                }
            }
        }
        if new_rows.is_empty() {
            return Err(non_exhaustive_ctor(span, &ctor.name));
        }
        return elab_matrix(globals, ctx, &new_columns, &new_rows, expected, span);
    }

    let (enum_name, params) = match &*col_ty {
        Val::Ind(name, args) => (name.clone(), args.clone()),
        _ => {
            return Err(Error::at(
                span,
                "match: a constructor pattern requires an enum value",
            ));
        }
    };
    let info = globals
        .get_enum(&enum_name)
        .ok_or_else(|| Error::new(format!("unknown enum `{}`", enum_name)))?
        .clone();
    let refinements: Vec<check::CtorRefinement> = info
        .ctors
        .iter()
        .map(|ctor| check::refine_ctor(globals, ctx, ctor, &params, &col_ty))
        .collect::<Result<_>>()?;
    let column_tm = quote(globals, ctx.level(), &column.value);
    let column_value = column.value.clone();

    let (motive, motive_v, mut bodies) = match expected {
        Some(exp) => {
            let motive = build_motive(globals, ctx, &column_tm, &col_ty, exp);
            let motive_v = eval(globals, &ctx.env, &motive);
            let mut bodies: Vec<Option<RcTm>> = vec![None; info.ctors.len()];
            for (idx, ctor) in info.ctors.iter().enumerate() {
                if !refinements[idx].possible {
                    continue;
                }
                let target = CtorTarget {
                    enum_name: &enum_name,
                    params: &params,
                    idx,
                    ctor,
                    refinement: &refinements[idx],
                };
                let (mut branch, branch_columns, branch_rows, ctor_v) =
                    specialize_ctor(globals, ctx, &target, columns, &rows)?;
                if branch_rows.is_empty() {
                    return Err(non_exhaustive_ctor(span, &ctor.name));
                }
                let branch_ty = vapp(globals, motive_v.clone(), ctor_v);
                let branch_ty = check::apply_solutions(
                    globals,
                    branch.ctx.level(),
                    &branch.vars,
                    &branch.solutions,
                    &branch_ty,
                );
                let (body, _) = elab_matrix(
                    globals,
                    &mut branch.ctx,
                    &branch_columns,
                    &branch_rows,
                    Some(&branch_ty),
                    span,
                )?;
                let body = substitute_field_solutions(
                    globals,
                    branch.ctx.level(),
                    ctor.num_fields,
                    &refinements[idx].field_solutions,
                    body,
                );
                bodies[idx] = Some(body);
            }
            (motive, motive_v, bodies)
        }
        None => {
            let mut inferred: Option<V> = None;
            let mut bodies: Vec<Option<RcTm>> = vec![None; info.ctors.len()];
            for (idx, ctor) in info.ctors.iter().enumerate() {
                if !refinements[idx].possible {
                    continue;
                }
                let target = CtorTarget {
                    enum_name: &enum_name,
                    params: &params,
                    idx,
                    ctor,
                    refinement: &refinements[idx],
                };
                let (mut branch, branch_columns, branch_rows, _) =
                    specialize_ctor(globals, ctx, &target, columns, &rows)?;
                if branch_rows.is_empty() {
                    return Err(non_exhaustive_ctor(span, &ctor.name));
                }
                let (body, body_ty) = elab_matrix(
                    globals,
                    &mut branch.ctx,
                    &branch_columns,
                    &branch_rows,
                    None,
                    span,
                )?;
                match &inferred {
                    None => inferred = Some(body_ty),
                    Some(t) => {
                        if !conv_ty(globals, branch.ctx.level(), t, &body_ty) {
                            return Err(Error::at(
                                span,
                                "match arms have different result types; add an expected type",
                            ));
                        }
                    }
                }
                let body = substitute_field_solutions(
                    globals,
                    branch.ctx.level(),
                    ctor.num_fields,
                    &refinements[idx].field_solutions,
                    body,
                );
                bodies[idx] = Some(body);
            }
            let rt = inferred.ok_or_else(|| {
                Error::at(
                    span,
                    "cannot infer the result type of a match with no reachable branches; add an expected type",
                )
            })?;
            let dom = quote(globals, ctx.level(), &col_ty);
            let body = quote(globals, ctx.level() + 1, &rt);
            let motive = lam("_", dom, body);
            let motive_v = eval(globals, &ctx.env, &motive);
            (motive, motive_v, bodies)
        }
    };

    let mut branches = Vec::with_capacity(info.ctors.len());
    for (idx, ctor) in info.ctors.iter().enumerate() {
        if !refinements[idx].possible {
            branches.push(Branch {
                ctor: ctor.name.clone(),
                binders: ctor.num_fields,
                body: rc(Tm::Prop),
                impossible: true,
            });
            continue;
        }
        let body = bodies[idx]
            .take()
            .ok_or_else(|| Error::new("internal: reachable constructor has no body"))?;
        branches.push(Branch {
            ctor: ctor.name.clone(),
            binders: ctor.num_fields,
            body,
            impossible: false,
        });
    }

    let tm = rc(Tm::Match {
        motive,
        scrut: column_tm,
        branches,
    });
    Ok((tm, vapp(globals, motive_v, column_value)))
}

/// Specialize a matrix by one constructor of its first column. Returns the
/// refined branch context, the new columns, the surviving rows, and the
/// constructor value used by the branch motive.
fn specialize_ctor<'a>(
    globals: &Globals,
    ctx: &Ctx,
    target: &CtorTarget<'_>,
    columns: &[MatchColumn],
    rows: &[MatchRow<'a>],
) -> Result<(BranchCtx, Vec<MatchColumn>, Vec<MatchRow<'a>>, V)> {
    let ctor = target.ctor;
    let refinement = target.refinement;
    let names: Vec<String> = (0..ctor.num_fields).map(|j| format!("_f{}", j)).collect();
    let branch = refine_branch(globals, ctx, refinement, &names)?;
    let inner = ctx.level();
    let level = branch.ctx.level();

    let mut field_vals = Vec::with_capacity(ctor.num_fields);
    for (j, solution) in refinement.field_solutions.iter().enumerate() {
        match solution {
            Some(solution) => field_vals.push(solution.clone()),
            None => field_vals.push(vvar(inner + j)),
        }
    }
    let mut args = target.params.to_vec();
    args.extend(field_vals);
    let ctor_v = vctor(target.enum_name.to_string(), target.idx, args);

    let mut new_columns = Vec::with_capacity(ctor.num_fields + columns.len() - 1);
    for j in 0..ctor.num_fields {
        let value = match &refinement.field_solutions[j] {
            Some(solution) => solution.clone(),
            None => vvar(inner + j),
        };
        new_columns.push(MatchColumn {
            ty: branch.ctx.locals[inner + j].1.clone(),
            value,
        });
    }
    for column in &columns[1..] {
        let ty =
            check::apply_solutions(globals, level, &branch.vars, &branch.solutions, &column.ty);
        let value = check::apply_solutions(
            globals,
            level,
            &branch.vars,
            &branch.solutions,
            &column.value,
        );
        new_columns.push(MatchColumn { ty, value });
    }

    let refined_column_ty = check::apply_solutions(
        globals,
        level,
        &branch.vars,
        &branch.solutions,
        &columns[0].ty,
    );
    let refined_column_value = check::apply_solutions(
        globals,
        level,
        &branch.vars,
        &branch.solutions,
        &columns[0].value,
    );
    let mut new_rows = Vec::new();
    for row in rows {
        match &row.pats[0] {
            Pattern::Ctor(name, subpats, _) if name == &ctor.name => {
                let mut pats = subpats.clone();
                pats.extend_from_slice(&row.pats[1..]);
                new_rows.push(MatchRow {
                    pats,
                    arm: row.arm,
                    bindings: row.bindings.clone(),
                });
            }
            Pattern::Ctor(..) => {}
            Pattern::Var(name, _) => {
                let mut pats = vec![Pattern::Wild(row.pats[0].span()); ctor.num_fields];
                pats.extend_from_slice(&row.pats[1..]);
                let mut bindings = row.bindings.clone();
                bindings.push((
                    name.clone(),
                    refined_column_ty.clone(),
                    refined_column_value.clone(),
                ));
                new_rows.push(MatchRow {
                    pats,
                    arm: row.arm,
                    bindings,
                });
            }
            Pattern::Wild(_) => {
                let mut pats = vec![Pattern::Wild(row.pats[0].span()); ctor.num_fields];
                pats.extend_from_slice(&row.pats[1..]);
                new_rows.push(MatchRow {
                    pats,
                    arm: row.arm,
                    bindings: row.bindings.clone(),
                });
            }
        }
    }

    Ok((branch, new_columns, new_rows, ctor_v))
}

/// Recover the field types of a constructor applied to concrete parameters.
fn ctor_field_types(
    globals: &Globals,
    ctor: &CtorInfo,
    params: &[V],
    fields: &[V],
) -> Result<Vec<V>> {
    if fields.len() != ctor.num_fields {
        return Err(Error::new("internal: constructor field mismatch"));
    }
    let mut ty = eval(globals, &Vec::new(), &ctor.ty);
    for param in params {
        ty = apply_pi(globals, ty, param.clone())
            .ok_or_else(|| Error::new("match: constructor parameter mismatch"))?;
    }
    let mut out = Vec::with_capacity(fields.len());
    for field in fields {
        let forced = force(globals, ty);
        let (dom, closure) = match &*forced {
            Val::Pi(dom, closure, _) => (dom.clone(), closure.clone()),
            _ => return Err(Error::new("match: constructor field mismatch")),
        };
        out.push(dom);
        ty = eval_closure(globals, &closure, field.clone());
    }
    Ok(out)
}

/// Treat a bare identifier that names a nullary constructor as a constructor
/// pattern, and validate constructor names and arities for this column.
fn normalize_pattern_column(
    globals: &Globals,
    col_ty: &V,
    rows: &mut [MatchRow<'_>],
) -> Result<()> {
    let info = match &**col_ty {
        Val::Ind(name, _) => Some(
            globals
                .get_enum(name)
                .ok_or_else(|| Error::new(format!("unknown enum `{}`", name)))?
                .clone(),
        ),
        _ => None,
    };
    for row in rows {
        if let Pattern::Var(name, span) = &row.pats[0]
            && let Some(info) = &info
            && info
                .ctors
                .iter()
                .any(|ctor| ctor.name == *name && ctor.num_fields == 0)
        {
            row.pats[0] = Pattern::Ctor(name.clone(), Vec::new(), *span);
        }
        if let Pattern::Ctor(name, subpats, span) = &row.pats[0] {
            let Some(info) = &info else {
                return Err(Error::at(
                    *span,
                    "match: a constructor pattern requires an enum value",
                ));
            };
            let ctor = info
                .ctors
                .iter()
                .find(|ctor| ctor.name == *name)
                .ok_or_else(|| {
                    Error::at(
                        *span,
                        format!("`{}` is not a constructor of `{}`", name, info.name),
                    )
                })?;
            if subpats.len() != ctor.num_fields {
                return Err(Error::at(
                    *span,
                    format!(
                        "constructor `{}` has {} field(s), but the pattern binds {}",
                        name,
                        ctor.num_fields,
                        subpats.len()
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn elab_row_body<'a>(
    globals: &Globals,
    ctx: &mut Ctx,
    row: &MatchRow<'a>,
    expected: Option<&V>,
) -> Result<(RcTm, V)> {
    let mut body_ctx = ctx.clone();
    for (name, ty, value) in &row.bindings {
        let level = body_ctx.level();
        body_ctx.push(name.clone(), ty.clone());
        // During elaboration the alias must be definitionally equal to the
        // value it will be replaced with after the body is checked.
        body_ctx.env[level] = value.clone();
    }
    let (body, body_ty) = match expected {
        Some(exp) => (
            elab_check(globals, &mut body_ctx, &row.arm.body, exp)?,
            exp.clone(),
        ),
        None => elab_infer(globals, &mut body_ctx, &row.arm.body)?,
    };
    let body = strip_bindings(globals, body_ctx.level(), &row.bindings, body);
    let body_ty = if expected.is_some() {
        body_ty
    } else {
        let ty_tm = quote(globals, body_ctx.level(), &body_ty);
        let ty_tm = strip_bindings(globals, body_ctx.level(), &row.bindings, ty_tm);
        let env: Env = (0..ctx.level()).map(vvar).collect();
        eval(globals, &env, &ty_tm)
    };
    Ok((body, body_ty))
}

/// Beta-reduce the variables introduced for irrefutable patterns. Aliases are
/// removed from innermost to outermost; each replacement is quoted in the
/// context that remains after the inner aliases are gone.
fn strip_bindings(
    globals: &Globals,
    mut level: usize,
    bindings: &[(String, V, V)],
    mut tm: RcTm,
) -> RcTm {
    for (_, _, value) in bindings.iter().rev() {
        level -= 1;
        let replacement = quote(globals, level, value);
        tm = instantiate(&tm, &replacement);
    }
    tm
}

/// Beta-reduce one binder: `tm` lives in `Gamma, x` and `arg` in `Gamma`.
fn instantiate(tm: &RcTm, arg: &RcTm) -> RcTm {
    let arg = shift(arg, 1, 0);
    let tm = crate::core::subst(tm, 0, &arg);
    shift(&tm, -1, 0)
}

fn non_exhaustive_ctor(span: Span, ctor: &str) -> Error {
    Error::at(
        span,
        format!("match is not exhaustive: missing constructor `{}`", ctor),
    )
}

/// Replace the branch-local variables of fields that the index refinement
/// solved, so the checked body mentions the same values used to build the
/// constructor in the motive. Runs a few passes to resolve solution chains.
fn substitute_field_solutions(
    globals: &Globals,
    level: usize,
    num_fields: usize,
    solutions: &[Option<V>],
    body: RcTm,
) -> RcTm {
    let mut body = body;
    if solutions.iter().all(|s| s.is_none()) {
        return body;
    }
    for _ in 0..=num_fields {
        for (j, solution) in solutions.iter().enumerate() {
            if let Some(solution) = solution {
                let index = num_fields - 1 - j;
                let replacement = quote(globals, level, solution);
                body = crate::core::subst(&body, index, &replacement);
            }
        }
    }
    body
}

fn build_motive(globals: &Globals, ctx: &Ctx, scrut_tm: &RcTm, scrut_ty: &V, expected: &V) -> RcTm {
    let level = ctx.level();
    let dom = quote(globals, level, scrut_ty);
    let target = shift(scrut_tm, 1, 0);
    let body = abstract_term(&target, &var(0), &quote(globals, level + 1, expected));
    lam("_", dom, body)
}

struct EqMatchData {
    proof: RcTm,
    ty: V,
    lhs: V,
    rhs: V,
}

fn elab_eq_match(
    globals: &Globals,
    ctx: &mut Ctx,
    eq: EqMatchData,
    arms: &[Arm],
    expected: Option<&V>,
    span: Span,
) -> Result<(RcTm, V)> {
    let EqMatchData {
        proof,
        ty: eq_ty,
        lhs: eq_lhs,
        rhs: eq_rhs,
    } = eq;
    if arms.len() != 1 {
        return Err(Error::at(
            span,
            "a match on an equality proof takes one arm",
        ));
    }
    match &arms[0].pattern {
        Pattern::Var(name, _) if name == "refl" => {}
        Pattern::Wild(_) => {}
        _ => {
            return Err(Error::at(
                arms[0].span,
                "a match on an equality proof must use the pattern `refl`",
            ));
        }
    }
    let ty_tm = quote(globals, ctx.level(), &eq_ty);
    let motive = match expected {
        Some(exp) => abstract_rhs(globals, ctx, &ty_tm, &eq_rhs, exp),
        None => {
            let (_, body_ty) = elab_infer(globals, ctx, &arms[0].body)?;
            let body_tm = quote(globals, ctx.level(), &body_ty);
            lam("_", ty_tm.clone(), body_tm)
        }
    };
    let motive_v = eval(globals, &ctx.env, &motive);
    let refl_expected = vapp(globals, motive_v.clone(), eq_lhs.clone());
    let refl_case = elab_check(globals, ctx, &arms[0].body, &refl_expected)?;
    let result = vapp(globals, motive_v, eq_rhs.clone());
    let tm = rc(Tm::J {
        ty: ty_tm,
        lhs: quote(globals, ctx.level(), &eq_lhs),
        rhs: quote(globals, ctx.level(), &eq_rhs),
        proof,
        motive,
        refl_case,
    });
    Ok((tm, result))
}

/// Build the motive for equality elimination: `fun (_: ty) => result`, with
/// occurrences of the right-hand side abstracted to the fresh variable.
fn abstract_rhs(globals: &Globals, ctx: &Ctx, ty_tm: &RcTm, rhs: &V, result: &V) -> RcTm {
    let level = ctx.level();
    let target = quote(globals, level + 1, rhs);
    let body = abstract_term(&target, &var(0), &quote(globals, level + 1, result));
    lam("_", ty_tm.clone(), body)
}
