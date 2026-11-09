use crate::check::{self, Ctx};
use crate::core::{Branch, RcTm, Tm, app, ctor, eq, ind, lam, pi, rc};
use crate::env::Globals;
use crate::error::{Error, Result};
use crate::eval::eval;

/// The relation of a variable to one function parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rel {
    /// The variable is the parameter itself.
    Equal,
    /// The variable is a strict structural subterm of the parameter.
    Less,
    /// Nothing is known.
    Unknown,
}

type Rels = Vec<Rel>;

fn unknown(arity: usize) -> Rels {
    vec![Rel::Unknown; arity]
}

/// Check that a definition is structurally recursive and that its recursive
/// name never escapes as a value.
///
/// The rule is deliberately small:
///
/// 1. every occurrence of the recursive name is the head of an application
///    spine of exactly `arity` arguments;
/// 2. every such call has a strictly structurally smaller argument in one
///    fixed parameter position (the same position for all calls).
///
/// Both parts are necessary. The first stops indirect recursion through an
/// escaped function value; the second stops direct non-decreasing recursion.
pub fn check_def(globals: &Globals, name: &str, body: &RcTm) -> Result<()> {
    let arity = globals
        .get_fn_num_params(name)
        .ok_or_else(|| Error::new(format!("unknown function `{}`", name)))?;
    let body = inline_lets(body);
    check_occurrences(name, arity, &body)?;
    check_decreasing(name, arity, &body)
}

/// Re-check every committed definition in the program. Definitions are sealed
/// while their own body is checked, so the checker never relies on the body it
/// is trying to justify.
pub fn check_program(globals: &Globals) -> Result<()> {
    check::check_enums(globals)?;
    for name in globals.fn_names() {
        let Some(body) = globals.get_fn_body(&name) else {
            continue;
        };
        let sealed = globals.sealed(&[&name]);
        check_def(&sealed, &name, body)?;

        let ty = sealed
            .get_ty(&name)
            .ok_or_else(|| Error::new(format!("unknown type for `{}`", name)))?;
        let mut type_ctx = Ctx::new();
        check::infer_sort(&sealed, &mut type_ctx, ty).map_err(|e| {
            Error::new(format!(
                "kernel rejected the type of `{}`: {}",
                name, e.message
            ))
        })?;
        let ty_v = eval(&sealed, &Vec::new(), ty);
        let mut ctx = Ctx::new();
        check::check(&sealed, &mut ctx, body, &ty_v)
            .map_err(|e| Error::new(format!("kernel rejected `{}`: {}", name, e.message)))?;
    }
    Ok(())
}

/// Beta-reduce single-use `let`/block bindings for checking. The surface
/// elaborator represents blocks and variable patterns as applied lambdas, so
/// this exposes the underlying variables and calls without duplicating terms.
///
/// This pass is not part of the trusted evaluator: it only canonicalises a
/// well-typed term before the structural checks.
fn inline_lets(tm: &RcTm) -> RcTm {
    match &**tm {
        Tm::App(f, a) => {
            let f = inline_lets(f);
            let a = inline_lets(a);
            if let Tm::Lam { body, .. } = &*f
                && count_var(body, 0, 0) == 1
            {
                let reduced = crate::core::subst(body, 0, &a);
                return inline_lets(&reduced);
            }
            app(f, a)
        }
        Tm::Lam { name, dom, body } => lam(name.clone(), inline_lets(dom), inline_lets(body)),
        Tm::Pi { name, dom, cod } => pi(name.clone(), inline_lets(dom), inline_lets(cod)),
        Tm::Ind(name, args) => ind(name.clone(), args.iter().map(inline_lets).collect()),
        Tm::Ctor(name, index, args) => {
            ctor(name.clone(), *index, args.iter().map(inline_lets).collect())
        }
        Tm::Match {
            motive,
            scrut,
            branches,
        } => rc(Tm::Match {
            motive: inline_lets(motive),
            scrut: inline_lets(scrut),
            branches: branches
                .iter()
                .map(|b| Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: inline_lets(&b.body),
                    impossible: b.impossible,
                })
                .collect(),
        }),
        Tm::Eq(a, b, c) => eq(inline_lets(a), inline_lets(b), inline_lets(c)),
        Tm::Refl(a) => rc(Tm::Refl(inline_lets(a))),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => rc(Tm::J {
            ty: inline_lets(ty),
            lhs: inline_lets(lhs),
            rhs: inline_lets(rhs),
            proof: inline_lets(proof),
            motive: inline_lets(motive),
            refl_case: inline_lets(refl_case),
        }),
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop => tm.clone(),
    }
}

fn count_var(tm: &RcTm, index: usize, depth: usize) -> usize {
    match &**tm {
        Tm::Var(i) if *i == index + depth => 1,
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop => 0,
        Tm::Pi { dom, cod, .. } => count_var(dom, index, depth) + count_var(cod, index, depth + 1),
        Tm::Lam { dom, body, .. } => {
            count_var(dom, index, depth) + count_var(body, index, depth + 1)
        }
        Tm::App(f, a) => count_var(f, index, depth) + count_var(a, index, depth),
        Tm::Ind(_, args) | Tm::Ctor(_, _, args) => {
            args.iter().map(|a| count_var(a, index, depth)).sum()
        }
        Tm::Match {
            motive,
            scrut,
            branches,
        } => {
            count_var(motive, index, depth)
                + count_var(scrut, index, depth)
                + branches
                    .iter()
                    .map(|b| count_var(&b.body, index, depth + b.binders))
                    .sum::<usize>()
        }
        Tm::Eq(a, b, c) => {
            count_var(a, index, depth) + count_var(b, index, depth) + count_var(c, index, depth)
        }
        Tm::Refl(a) => count_var(a, index, depth),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => {
            count_var(ty, index, depth)
                + count_var(lhs, index, depth)
                + count_var(rhs, index, depth)
                + count_var(proof, index, depth)
                + count_var(motive, index, depth)
                + count_var(refl_case, index, depth)
        }
    }
}

fn spine(tm: &RcTm) -> (&RcTm, Vec<&RcTm>) {
    let mut args = Vec::new();
    let mut cur = tm;
    while let Tm::App(f, a) = &**cur {
        args.push(a);
        cur = f;
    }
    args.reverse();
    (cur, args)
}

fn check_occurrences(name: &str, arity: usize, tm: &RcTm) -> Result<()> {
    match &**tm {
        Tm::App(..) => {
            let (head, args) = spine(tm);
            if let Tm::Const(n) = &**head
                && n == name
            {
                if args.len() != arity {
                    return Err(Error::new(format!(
                        "recursive call to `{}` has {} argument(s), but it takes {}",
                        name,
                        args.len(),
                        arity
                    )));
                }
                for arg in args {
                    check_occurrences(name, arity, arg)?;
                }
                Ok(())
            } else {
                check_occurrences(name, arity, head)?;
                for arg in args {
                    check_occurrences(name, arity, arg)?;
                }
                Ok(())
            }
        }
        Tm::Const(n) if n == name => {
            if arity == 0 {
                // A nullary recursive reference is a nullary call. The
                // decreasing check rejects it because nothing can decrease.
                Ok(())
            } else {
                Err(Error::new(format!(
                    "recursive function `{}` is used as a value; recursive calls must be fully applied",
                    name
                )))
            }
        }
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop => Ok(()),
        Tm::Pi { dom, cod, .. } => {
            check_occurrences(name, arity, dom)?;
            check_occurrences(name, arity, cod)
        }
        Tm::Lam { dom, body, .. } => {
            check_occurrences(name, arity, dom)?;
            check_occurrences(name, arity, body)
        }
        Tm::Ind(_, args) | Tm::Ctor(_, _, args) => {
            for arg in args {
                check_occurrences(name, arity, arg)?;
            }
            Ok(())
        }
        Tm::Match {
            motive,
            scrut,
            branches,
        } => {
            check_occurrences(name, arity, motive)?;
            check_occurrences(name, arity, scrut)?;
            for branch in branches {
                check_occurrences(name, arity, &branch.body)?;
            }
            Ok(())
        }
        Tm::Eq(a, b, c) => {
            check_occurrences(name, arity, a)?;
            check_occurrences(name, arity, b)?;
            check_occurrences(name, arity, c)
        }
        Tm::Refl(a) => check_occurrences(name, arity, a),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => {
            check_occurrences(name, arity, ty)?;
            check_occurrences(name, arity, lhs)?;
            check_occurrences(name, arity, rhs)?;
            check_occurrences(name, arity, proof)?;
            check_occurrences(name, arity, motive)?;
            check_occurrences(name, arity, refl_case)
        }
    }
}

fn check_decreasing(name: &str, arity: usize, body: &RcTm) -> Result<()> {
    let mut calls = Vec::new();
    let (doms, inner) = peel_lams(body, arity)?;

    // Domains are checked conservatively: no parameter is known to be smaller
    // inside a later parameter's type.
    for (k, dom) in doms.iter().enumerate() {
        let env = vec![unknown(arity); k];
        check_decreasing_tm(name, arity, dom, &env, &mut calls)?;
    }

    let mut env = vec![unknown(arity); arity];
    for k in 0..arity {
        // At the innermost body, `Var(arity - 1 - k)` is parameter `k`.
        env[arity - 1 - k][k] = Rel::Equal;
    }
    check_decreasing_tm(name, arity, inner, &env, &mut calls)?;

    if calls.is_empty() {
        return Ok(());
    }

    let mut common: Option<Vec<usize>> = None;
    for smaller in calls {
        if smaller.is_empty() {
            return Err(Error::new(format!(
                "recursive call to `{}` is not structurally decreasing; no argument is a strict subterm of a parameter",
                name
            )));
        }
        common = Some(match common {
            None => smaller,
            Some(previous) => previous
                .into_iter()
                .filter(|i| smaller.contains(i))
                .collect(),
        });
        if common.as_ref().is_some_and(|v| v.is_empty()) {
            return Err(Error::new(format!(
                "recursive calls to `{}` do not decrease in a single consistent argument position",
                name
            )));
        }
    }
    Ok(())
}

fn peel_lams(mut tm: &RcTm, arity: usize) -> Result<(Vec<&RcTm>, &RcTm)> {
    let mut doms = Vec::new();
    for _ in 0..arity {
        match &**tm {
            Tm::Lam { dom, body, .. } => {
                doms.push(dom);
                tm = body;
            }
            _ => {
                return Err(Error::new(
                    "internal: function body does not have the expected parameters",
                ));
            }
        }
    }
    Ok((doms, tm))
}

fn check_decreasing_tm(
    name: &str,
    arity: usize,
    tm: &RcTm,
    env: &[Rels],
    calls: &mut Vec<Vec<usize>>,
) -> Result<()> {
    match &**tm {
        Tm::App(..) => {
            let (head, args) = spine(tm);
            if let Tm::Const(n) = &**head
                && n == name
            {
                if args.len() != arity {
                    return Err(Error::new(format!(
                        "recursive call to `{}` has {} argument(s), but it takes {}",
                        name,
                        args.len(),
                        arity
                    )));
                }
                let mut smaller = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    if i >= arity {
                        break;
                    }
                    let rels = size_of(arg, env, arity);
                    if rels[i] == Rel::Less {
                        smaller.push(i);
                    }
                }
                calls.push(smaller);
                for arg in &args {
                    check_decreasing_tm(name, arity, arg, env, calls)?;
                }
                Ok(())
            } else {
                check_decreasing_tm(name, arity, head, env, calls)?;
                for arg in &args {
                    check_decreasing_tm(name, arity, arg, env, calls)?;
                }
                Ok(())
            }
        }
        Tm::Const(n) if n == name => {
            // A nullary recursive call: there is no argument that can be
            // structurally smaller, so this can never be accepted.
            if arity == 0 {
                calls.push(Vec::new());
            }
            Ok(())
        }
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop => Ok(()),
        Tm::Pi { dom, cod, .. } => {
            check_decreasing_tm(name, arity, dom, env, calls)?;
            check_decreasing_tm(name, arity, cod, &push_unknown(env, arity), calls)
        }
        Tm::Lam { dom, body, .. } => {
            check_decreasing_tm(name, arity, dom, env, calls)?;
            check_decreasing_tm(name, arity, body, &push_unknown(env, arity), calls)
        }
        Tm::Ind(_, args) | Tm::Ctor(_, _, args) => {
            for arg in args {
                check_decreasing_tm(name, arity, arg, env, calls)?;
            }
            Ok(())
        }
        Tm::Match {
            motive,
            scrut,
            branches,
        } => {
            check_decreasing_tm(name, arity, motive, env, calls)?;
            check_decreasing_tm(name, arity, scrut, env, calls)?;
            let scrut_rels = size_of(scrut, env, arity);
            let field_rels = field_rels(&scrut_rels);
            for branch in branches {
                let mut inner = env.to_vec();
                for _ in 0..branch.binders {
                    inner.insert(0, field_rels.clone());
                }
                check_decreasing_tm(name, arity, &branch.body, &inner, calls)?;
            }
            Ok(())
        }
        Tm::Eq(a, b, c) => {
            check_decreasing_tm(name, arity, a, env, calls)?;
            check_decreasing_tm(name, arity, b, env, calls)?;
            check_decreasing_tm(name, arity, c, env, calls)
        }
        Tm::Refl(a) => check_decreasing_tm(name, arity, a, env, calls),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => {
            check_decreasing_tm(name, arity, ty, env, calls)?;
            check_decreasing_tm(name, arity, lhs, env, calls)?;
            check_decreasing_tm(name, arity, rhs, env, calls)?;
            check_decreasing_tm(name, arity, proof, env, calls)?;
            check_decreasing_tm(name, arity, motive, env, calls)?;
            check_decreasing_tm(name, arity, refl_case, env, calls)
        }
    }
}

fn push_unknown(env: &[Rels], arity: usize) -> Vec<Rels> {
    let mut inner = Vec::with_capacity(env.len() + 1);
    inner.push(unknown(arity));
    inner.extend_from_slice(env);
    inner
}

fn field_rels(scrut: &Rels) -> Rels {
    scrut
        .iter()
        .map(|rel| match rel {
            Rel::Equal | Rel::Less => Rel::Less,
            Rel::Unknown => Rel::Unknown,
        })
        .collect()
}

/// Compute the structural relation of an expression to the parameters. Only
/// variables and applied lambdas can have a known relation; everything else is
/// conservative.
fn size_of(tm: &RcTm, env: &[Rels], arity: usize) -> Rels {
    match &**tm {
        Tm::Var(i) => env.get(*i).cloned().unwrap_or_else(|| unknown(arity)),
        Tm::App(f, a) => {
            if let Tm::Lam { body, .. } = &**f {
                let arg = size_of(a, env, arity);
                let mut inner = Vec::with_capacity(env.len() + 1);
                inner.push(arg);
                inner.extend_from_slice(env);
                size_of(body, &inner, arity)
            } else {
                unknown(arity)
            }
        }
        _ => unknown(arity),
    }
}
