use std::rc::Rc;

use crate::core::{Branch, PrimLit, PrimOp, PrimOutcome, PrimRet, RcTm};
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
        PrimTy(t) => vprimty(*t),
        PrimLit(lit) => vprimlit(lit.clone()),
        PrimOp(op, args) => {
            let args: Vec<V> = args.iter().map(|a| eval(globals, env, a)).collect();
            reduce_prim(globals, *op, &args)
        }
        Pi {
            dom, cod, implicit, ..
        } => {
            let dom_v = eval(globals, env, dom);
            let closure = Closure {
                dom: None,
                env: env.clone(),
                body: cod.clone(),
            };
            vpi(dom_v, closure, *implicit)
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
                    let branch = match branches.get(*index) {
                        Some(b) if !b.impossible => b,
                        _ => {
                            return vneutral(Neutral::Match {
                                env: env.clone(),
                                motive: motive.clone(),
                                scrut: forced.clone(),
                                branches: branches.clone(),
                            });
                        }
                    };
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
            motive,
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
                    motive: motive.clone(),
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

/// Reduce a primitive operation whose arguments are already values. The
/// operation reduces when every argument is a literal of the expected type;
/// otherwise it stays neutral.
pub fn reduce_prim(globals: &Globals, op: PrimOp, args: &[V]) -> V {
    let forced: Vec<V> = args.iter().map(|a| force(globals, a.clone())).collect();
    let literals: Option<Vec<PrimLit>> = forced
        .iter()
        .map(|v| match &**v {
            Val::PrimLit(lit) => Some(lit.clone()),
            _ => None,
        })
        .collect();
    if let Some(literals) = literals
        && op.accepts(&literals)
        && let Some(value) = prim_outcome(globals, op.result(), op.apply(&literals))
    {
        return value;
    }
    vprimop(op, forced)
}

fn prim_outcome(globals: &Globals, result: PrimRet, outcome: PrimOutcome) -> Option<V> {
    match outcome {
        PrimOutcome::U32(n) => Some(vprimlit(PrimLit::U32(n))),
        PrimOutcome::Bool(flag) => {
            let (enum_name, index) = globals.bool_ctor(flag)?;
            Some(vctor(enum_name, index, Vec::new()))
        }
        PrimOutcome::Bytes(bytes) => {
            let lit = if result == PrimRet::Str {
                PrimLit::Str(bytes)
            } else {
                PrimLit::Bytes(bytes)
            };
            Some(vprimlit(lit))
        }
    }
}

pub fn apply_pi(globals: &Globals, pi: V, arg: V) -> Option<V> {
    let pi = force(globals, pi);
    match &*pi {
        Val::Pi(_, closure, _) => Some(eval_closure(globals, closure, arg)),
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
                Some(b) if !b.impossible => b,
                _ => {
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
    pub motive: &'a RcTm,
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
            motive: args.motive.clone(),
            refl_case: args.refl_case.clone(),
        }),
    }
}
