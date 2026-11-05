use std::rc::Rc;

use crate::core::{Branch, RcTm};
use crate::env::{GlobalDef, Globals};
use crate::value::*;

pub fn eval(globals: &Globals, env: &Env, tm: &RcTm) -> V {
    use crate::core::Tm::*;
    match &**tm {
        Var(i) => {
            let n = env.len();
            env[n - 1 - i].clone()
        }
        Const(name) => match globals.defs.get(name) {
            Some(GlobalDef::Ctor {
                enum_name, index, ..
            }) => vctor(enum_name.clone(), *index, Vec::new()),
            _ => vglob(name.clone()),
        },
        Type(u) => vtype(*u),
        Prop => vprop(),
        Pi { dom, cod, .. } => {
            let dom_v = eval(globals, env, dom);
            let closure = Closure {
                dom: None,
                env: env.clone(),
                body: cod.clone(),
            };
            vpi(dom_v, closure)
        }
        Lam { dom, body, .. } => {
            let closure = Closure {
                dom: Some(dom.clone()),
                env: env.clone(),
                body: body.clone(),
            };
            vlam(closure)
        }
        App(f, a) => {
            let fv = eval(globals, env, f);
            let av = eval(globals, env, a);
            vapp(globals, fv, av)
        }
        Ind(name, args) => {
            let args = args.iter().map(|a| eval(globals, env, a)).collect();
            vind(name.clone(), args)
        }
        Ctor(enum_name, index, args) => {
            let args = args.iter().map(|a| eval(globals, env, a)).collect();
            vctor(enum_name.clone(), *index, args)
        }
        Match {
            motive,
            scrut,
            branches,
        } => {
            let scrut_v = eval(globals, env, scrut);
            let forced = force(globals, scrut_v.clone());
            match &*forced {
                Val::Ctor(enum_name, index, args) => {
                    let branch = branches.get(*index).expect("missing constructor branch");
                    let field_count = branch.binders;
                    let mut env2 = env.clone();
                    let start = args.len().saturating_sub(field_count);
                    for v in &args[start..] {
                        env2.push(v.clone());
                    }
                    let _ = enum_name;
                    eval(globals, &env2, &branch.body)
                }
                _ => vneutral(Neutral::Match {
                    env: env.clone(),
                    motive: motive.clone(),
                    scrut: forced,
                    branches: branches.clone(),
                }),
            }
        }
        Eq(ty, lhs, rhs) => {
            let ty = eval(globals, env, ty);
            let lhs = eval(globals, env, lhs);
            let rhs = eval(globals, env, rhs);
            veq(ty, lhs, rhs)
        }
        Refl(x) => {
            let x = eval(globals, env, x);
            vrefl(x)
        }
        J {
            ty,
            lhs,
            rhs,
            proof,
            result_ty,
            refl_case,
        } => {
            let proof_v = eval(globals, env, proof);
            let forced = force(globals, proof_v.clone());
            match &*forced {
                Val::Refl(_) => eval(globals, env, refl_case),
                _ => vneutral(Neutral::J {
                    env: env.clone(),
                    ty: ty.clone(),
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    proof: forced,
                    result_ty: result_ty.clone(),
                    refl_case: refl_case.clone(),
                }),
            }
        }
    }
}

pub fn eval_closure(globals: &Globals, closure: &Closure, arg: V) -> V {
    let mut env = closure.env.clone();
    env.push(arg);
    eval(globals, &env, &closure.body)
}

pub fn apply_pi(globals: &Globals, pi: V, arg: V) -> Option<V> {
    let pi = force(globals, pi);
    match &*pi {
        Val::Pi(_, closure) => Some(eval_closure(globals, closure, arg)),
        _ => None,
    }
}

pub fn force(globals: &Globals, v: V) -> V {
    match &*v {
        Val::Glob(name) => {
            if let Some(body) = globals.get_fn_body(name) {
                eval(globals, &Vec::new(), body)
            } else {
                v.clone()
            }
        }
        _ => v.clone(),
    }
}

pub fn vapp(globals: &Globals, f: V, a: V) -> V {
    let f = force(globals, f);
    match &*f {
        Val::Lam(closure) => eval_closure(globals, closure, a),
        Val::Glob(name) => {
            if let Some(body) = globals.get_fn_body(name) {
                let unfolded = eval(globals, &Vec::new(), body);
                vapp(globals, unfolded, a)
            } else {
                vneutral(Neutral::App(Rc::new(Neutral::Global(name.clone())), a))
            }
        }
        Val::Ctor(enum_name, index, args) => {
            let arity = globals.ctor_arity(enum_name, *index).unwrap_or(args.len());
            if args.len() < arity {
                let mut args = args.clone();
                args.push(a);
                vctor(enum_name.clone(), *index, args)
            } else {
                f.clone()
            }
        }
        Val::Neutral(n) => vneutral(Neutral::App(n.clone(), a)),
        _ => f.clone(),
    }
}

/// Reduce a match whose scrutinee is already a value.
pub fn reduce_match(
    globals: &Globals,
    env: &Env,
    motive: &RcTm,
    scrut: &V,
    branches: &[Branch],
) -> V {
    let forced = force(globals, scrut.clone());
    match &*forced {
        Val::Ctor(enum_name, index, args) => {
            let branch = match branches.get(*index) {
                Some(b) => b,
                None => {
                    return vneutral(Neutral::Match {
                        env: env.clone(),
                        motive: motive.clone(),
                        scrut: forced,
                        branches: branches.to_vec(),
                    });
                }
            };
            let num_params = globals
                .get_enum(enum_name)
                .and_then(|e| e.ctors.get(*index))
                .map(|c| c.num_params)
                .unwrap_or(0);
            let mut env2 = env.clone();
            env2.extend(args[num_params.min(args.len())..].iter().cloned());
            eval(globals, &env2, &branch.body)
        }
        _ => vneutral(Neutral::Match {
            env: env.clone(),
            motive: motive.clone(),
            scrut: forced,
            branches: branches.to_vec(),
        }),
    }
}

/// The term arguments of an equality elimination.
pub struct JArgs<'a> {
    pub ty: &'a RcTm,
    pub lhs: &'a RcTm,
    pub rhs: &'a RcTm,
    pub result_ty: &'a RcTm,
    pub refl_case: &'a RcTm,
}

/// Reduce an equality elimination whose proof is already a value.
pub fn reduce_j(globals: &Globals, env: &Env, proof: &V, args: &JArgs<'_>) -> V {
    let forced = force(globals, proof.clone());
    match &*forced {
        Val::Refl(_) => eval(globals, env, args.refl_case),
        _ => vneutral(Neutral::J {
            env: env.clone(),
            ty: args.ty.clone(),
            lhs: args.lhs.clone(),
            rhs: args.rhs.clone(),
            proof: forced,
            result_ty: args.result_ty.clone(),
            refl_case: args.refl_case.clone(),
        }),
    }
}
