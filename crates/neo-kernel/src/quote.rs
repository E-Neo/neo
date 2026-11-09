use std::rc::Rc;

use crate::core::{Branch, RcTm, Tm};
use crate::env::Globals;
use crate::eval::{eval_closure, force};
use crate::value::*;

pub fn quote(globals: &Globals, level: usize, v: &V) -> RcTm {
    let v = force(globals, v.clone());
    match &*v {
        Val::Type(u) => Rc::new(Tm::Type(*u)),
        Val::Prop => Rc::new(Tm::Prop),
        Val::Pi(dom, closure) => {
            let dom_tm = quote(globals, level, dom);
            let fresh = vvar(level);
            let cod_v = eval_closure(globals, closure, fresh);
            let cod_tm = quote(globals, level + 1, &cod_v);
            Rc::new(Tm::Pi {
                name: "x".to_string(),
                dom: dom_tm,
                cod: cod_tm,
            })
        }
        Val::Lam(closure) => {
            let dom_tm = closure.dom.clone().unwrap_or_else(|| Rc::new(Tm::Type(0)));
            let fresh = vvar(level);
            let body_v = eval_closure(globals, closure, fresh);
            let body_tm = quote(globals, level + 1, &body_v);
            Rc::new(Tm::Lam {
                name: "x".to_string(),
                dom: dom_tm,
                body: body_tm,
            })
        }
        Val::Neutral(n) => quote_neutral(globals, level, n),
        Val::Ind(name, args) => Rc::new(Tm::Ind(
            name.clone(),
            args.iter().map(|a| quote(globals, level, a)).collect(),
        )),
        Val::Ctor(enum_name, index, args) => Rc::new(Tm::Ctor(
            enum_name.clone(),
            *index,
            args.iter().map(|a| quote(globals, level, a)).collect(),
        )),
        Val::Eq(ty, lhs, rhs) => Rc::new(Tm::Eq(
            quote(globals, level, ty),
            quote(globals, level, lhs),
            quote(globals, level, rhs),
        )),
        Val::Refl(x) => Rc::new(Tm::Refl(quote(globals, level, x))),
        Val::Glob(name) => Rc::new(Tm::Const(name.clone())),
    }
}

fn quote_neutral(globals: &Globals, level: usize, n: &Neutral) -> RcTm {
    match n {
        Neutral::Var(l) => {
            if *l >= level {
                Rc::new(Tm::Var(0))
            } else {
                Rc::new(Tm::Var(level - 1 - l))
            }
        }
        Neutral::Global(name) => Rc::new(Tm::Const(name.clone())),
        Neutral::App(f, a) => Rc::new(Tm::App(
            quote_neutral(globals, level, f),
            quote(globals, level, a),
        )),
        Neutral::Match {
            env,
            motive,
            scrut,
            branches,
        } => {
            let motive_tm = quote_open(globals, level, env, motive);
            let scrut_tm = quote(globals, level, scrut);
            let mut out = Vec::new();
            for b in branches {
                out.push(Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: quote_open_binders(globals, level, env, b.binders, &b.body),
                    impossible: b.impossible,
                });
            }
            Rc::new(Tm::Match {
                motive: motive_tm,
                scrut: scrut_tm,
                branches: out,
            })
        }
        Neutral::J {
            env,
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => Rc::new(Tm::J {
            ty: quote_open(globals, level, env, ty),
            lhs: quote_open(globals, level, env, lhs),
            rhs: quote_open(globals, level, env, rhs),
            proof: quote(globals, level, proof),
            motive: quote_open(globals, level, env, motive),
            refl_case: quote_open(globals, level, env, refl_case),
        }),
    }
}

/// Substitute the values in `env` for the free variables of `tm`, producing a
/// term in an ambient context of length `level`.
pub fn quote_open(globals: &Globals, level: usize, env: &Env, tm: &RcTm) -> RcTm {
    close_under(globals, level, env, tm, 0)
}

/// Like `quote_open`, for a term already under `binders` internal binders.
pub fn quote_open_binders(
    globals: &Globals,
    level: usize,
    env: &Env,
    binders: usize,
    tm: &RcTm,
) -> RcTm {
    close_under(globals, level + binders, env, tm, binders)
}

fn close_under(globals: &Globals, level: usize, env: &Env, tm: &RcTm, extra: usize) -> RcTm {
    // `extra` is the number of binders inside `tm` that are already accounted
    // for by the caller; `env` is the substitution for variables below them.
    match &**tm {
        Tm::Var(i) => {
            if *i < extra {
                Rc::new(Tm::Var(*i))
            } else {
                let j = *i - extra;
                if j < env.len() {
                    quote(globals, level, &env[env.len() - 1 - j])
                } else {
                    // A dangling variable should not occur; keep it stable.
                    Rc::new(Tm::Var(*i))
                }
            }
        }
        Tm::Const(name) => Rc::new(Tm::Const(name.clone())),
        Tm::Type(u) => Rc::new(Tm::Type(*u)),
        Tm::Prop => Rc::new(Tm::Prop),
        Tm::Pi { name, dom, cod } => Rc::new(Tm::Pi {
            name: name.clone(),
            dom: close_under(globals, level, env, dom, extra),
            cod: close_under(globals, level + 1, env, cod, extra + 1),
        }),
        Tm::Lam { name, dom, body } => Rc::new(Tm::Lam {
            name: name.clone(),
            dom: close_under(globals, level, env, dom, extra),
            body: close_under(globals, level + 1, env, body, extra + 1),
        }),
        Tm::App(f, a) => Rc::new(Tm::App(
            close_under(globals, level, env, f, extra),
            close_under(globals, level, env, a, extra),
        )),
        Tm::Ind(name, args) => Rc::new(Tm::Ind(
            name.clone(),
            args.iter()
                .map(|a| close_under(globals, level, env, a, extra))
                .collect(),
        )),
        Tm::Ctor(name, index, args) => Rc::new(Tm::Ctor(
            name.clone(),
            *index,
            args.iter()
                .map(|a| close_under(globals, level, env, a, extra))
                .collect(),
        )),
        Tm::Match {
            motive,
            scrut,
            branches,
        } => Rc::new(Tm::Match {
            motive: close_under(globals, level, env, motive, extra),
            scrut: close_under(globals, level, env, scrut, extra),
            branches: branches
                .iter()
                .map(|b| Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: close_under(globals, level + b.binders, env, &b.body, extra + b.binders),
                    impossible: b.impossible,
                })
                .collect(),
        }),
        Tm::Eq(a, b, c) => Rc::new(Tm::Eq(
            close_under(globals, level, env, a, extra),
            close_under(globals, level, env, b, extra),
            close_under(globals, level, env, c, extra),
        )),
        Tm::Refl(a) => Rc::new(Tm::Refl(close_under(globals, level, env, a, extra))),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => Rc::new(Tm::J {
            ty: close_under(globals, level, env, ty, extra),
            lhs: close_under(globals, level, env, lhs, extra),
            rhs: close_under(globals, level, env, rhs, extra),
            proof: close_under(globals, level, env, proof, extra),
            motive: close_under(globals, level, env, motive, extra),
            refl_case: close_under(globals, level, env, refl_case, extra),
        }),
    }
}
