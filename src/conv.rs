use std::rc::Rc;

use crate::env::Globals;
use crate::eval::{eval_closure, force, vapp};
use crate::value::*;

pub fn conv(globals: &Globals, level: usize, a: &V, b: &V) -> bool {
    let a = force(globals, a.clone());
    let b = force(globals, b.clone());
    match (&*a, &*b) {
        (Val::Type(u), Val::Type(v)) => u == v,
        (Val::Prop, Val::Prop) => true,
        (Val::Pi(da, ca), Val::Pi(db, cb)) => {
            if !conv_ty(globals, level, da, db) {
                return false;
            }
            let fresh = vvar(level);
            let ca = eval_closure(globals, ca, fresh.clone());
            let cb = eval_closure(globals, cb, fresh);
            conv_ty(globals, level + 1, &ca, &cb)
        }
        (Val::Lam(ca), Val::Lam(cb)) => {
            let fresh = vvar(level);
            let a = eval_closure(globals, ca, fresh.clone());
            let b = eval_closure(globals, cb, fresh);
            conv(globals, level + 1, &a, &b)
        }
        (Val::Lam(ca), _) => {
            let fresh = vvar(level);
            let a = eval_closure(globals, ca, fresh.clone());
            let b = vapp(globals, b.clone(), fresh);
            conv(globals, level + 1, &a, &b)
        }
        (_, Val::Lam(cb)) => {
            let fresh = vvar(level);
            let a = vapp(globals, a.clone(), fresh.clone());
            let b = eval_closure(globals, cb, fresh);
            conv(globals, level + 1, &a, &b)
        }
        (Val::Neutral(na), Val::Neutral(nb)) => conv_neutral(globals, level, na, nb),
        (Val::Ind(na, aa), Val::Ind(nb, ab)) => {
            na == nb
                && aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab.iter())
                    .all(|(x, y)| conv(globals, level, x, y))
        }
        (Val::Ctor(ea, ia, aa), Val::Ctor(eb, ib, ab)) => {
            ea == eb
                && ia == ib
                && aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab.iter())
                    .all(|(x, y)| conv(globals, level, x, y))
        }
        (Val::Eq(ta, la, ra), Val::Eq(tb, lb, rb)) => {
            conv(globals, level, ta, tb)
                && conv(globals, level, la, lb)
                && conv(globals, level, ra, rb)
        }
        (Val::Refl(a), Val::Refl(b)) => conv(globals, level, a, b),
        _ => false,
    }
}

pub fn conv_ty(globals: &Globals, level: usize, a: &V, b: &V) -> bool {
    let a = force(globals, a.clone());
    let b = force(globals, b.clone());
    match (&*a, &*b) {
        (Val::Type(u), Val::Type(v)) => u <= v,
        (Val::Prop, Val::Prop) => true,
        (Val::Pi(da, ca), Val::Pi(db, cb)) => {
            if !conv_ty(globals, level, da, db) {
                return false;
            }
            let fresh = vvar(level);
            let ca = eval_closure(globals, ca, fresh.clone());
            let cb = eval_closure(globals, cb, fresh);
            conv_ty(globals, level + 1, &ca, &cb)
        }
        (Val::Ind(na, aa), Val::Ind(nb, ab)) => {
            na == nb
                && aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab.iter())
                    .all(|(x, y)| conv(globals, level, x, y))
        }
        _ => conv(globals, level, &a, &b),
    }
}

pub fn is_prop(globals: &Globals, level: usize, ty: &V) -> bool {
    let ty = force(globals, ty.clone());
    match &*ty {
        Val::Prop => true,
        Val::Pi(_, closure) => {
            let fresh = vvar(level);
            let cod = eval_closure(globals, closure, fresh);
            is_prop(globals, level + 1, &cod)
        }
        Val::Ind(name, _) => globals.sort_of_enum(name) == Some(crate::env::Sort::Prop),
        Val::Eq(_, _, _) => true,
        _ => false,
    }
}

pub fn conv_at(globals: &Globals, level: usize, ty: &V, a: &V, b: &V) -> bool {
    if is_prop(globals, level, ty) {
        return true;
    }
    conv(globals, level, a, b)
}

fn conv_neutral(globals: &Globals, level: usize, a: &Neutral, b: &Neutral) -> bool {
    match (a, b) {
        (Neutral::Var(i), Neutral::Var(j)) => i == j,
        (Neutral::Global(x), Neutral::Global(y)) => x == y,
        (Neutral::App(f, x), Neutral::App(g, y)) => {
            conv_neutral(globals, level, f, g) && conv(globals, level, x, y)
        }
        (
            Neutral::Match {
                env: ea,
                motive: ma,
                scrut: sa,
                branches: ba,
            },
            Neutral::Match {
                env: eb,
                motive: mb,
                scrut: sb,
                branches: bb,
            },
        ) => {
            if ea.len() != eb.len() || ba.len() != bb.len() {
                return false;
            }
            if !conv(globals, level, sa, sb) {
                return false;
            }
            let norm = 1_000_000 + ea.len().max(eb.len());
            let ma_n = crate::quote::quote_open(globals, norm, ea, ma);
            let mb_n = crate::quote::quote_open(globals, norm, eb, mb);
            if !crate::core::same_tm(&ma_n, &mb_n) {
                return false;
            }
            for (x, y) in ba.iter().zip(bb.iter()) {
                if x.ctor != y.ctor || x.binders != y.binders {
                    return false;
                }
                let na = crate::quote::quote_open_binders(globals, norm, ea, x.binders, &x.body);
                let nb = crate::quote::quote_open_binders(globals, norm, eb, y.binders, &y.body);
                if !crate::core::same_tm(&na, &nb) {
                    return false;
                }
            }
            true
        }
        (
            Neutral::J {
                env: ea, proof: pa, ..
            },
            Neutral::J {
                env: eb, proof: pb, ..
            },
        ) => ea.len() == eb.len() && conv(globals, level, pa, pb),
        _ => false,
    }
}

pub fn replace_val(globals: &Globals, target: &V, replacement: &V, v: &V) -> V {
    if same_val(globals, v, target) {
        return replacement.clone();
    }
    let forced = force(globals, v.clone());
    match &*forced {
        Val::Neutral(n) => match &**n {
            Neutral::App(f, a) => {
                let f = replace_val(
                    globals,
                    target,
                    replacement,
                    &Rc::new(Val::Neutral(f.clone())),
                );
                let a = replace_val(globals, target, replacement, a);
                vapp(globals, f, a)
            }
            Neutral::Match {
                env,
                motive,
                scrut,
                branches,
            } => {
                let env: Env = env
                    .iter()
                    .map(|e| replace_val(globals, target, replacement, e))
                    .collect();
                let scrut = replace_val(globals, target, replacement, scrut);
                crate::eval::reduce_match(globals, &env, motive, &scrut, branches)
            }
            Neutral::J {
                env,
                ty,
                lhs,
                rhs,
                proof,
                result_ty,
                refl_case,
            } => {
                let env: Env = env
                    .iter()
                    .map(|e| replace_val(globals, target, replacement, e))
                    .collect();
                let proof = replace_val(globals, target, replacement, proof);
                crate::eval::reduce_j(
                    globals,
                    &env,
                    &proof,
                    &crate::eval::JArgs {
                        ty,
                        lhs,
                        rhs,
                        result_ty,
                        refl_case,
                    },
                )
            }
            _ => forced,
        },
        Val::Ind(name, args) => vind(
            name.clone(),
            args.iter()
                .map(|a| replace_val(globals, target, replacement, a))
                .collect(),
        ),
        Val::Ctor(enum_name, index, args) => vctor(
            enum_name.clone(),
            *index,
            args.iter()
                .map(|a| replace_val(globals, target, replacement, a))
                .collect(),
        ),
        Val::Eq(ty, lhs, rhs) => veq(
            replace_val(globals, target, replacement, ty),
            replace_val(globals, target, replacement, lhs),
            replace_val(globals, target, replacement, rhs),
        ),
        Val::Refl(x) => vrefl(replace_val(globals, target, replacement, x)),
        _ => forced,
    }
}

fn same_val(globals: &Globals, a: &V, b: &V) -> bool {
    let a = force(globals, a.clone());
    let b = force(globals, b.clone());
    match (&*a, &*b) {
        (Val::Type(u), Val::Type(v)) => u == v,
        (Val::Prop, Val::Prop) => true,
        (Val::Ind(na, aa), Val::Ind(nb, ab)) => {
            na == nb
                && aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab.iter())
                    .all(|(x, y)| same_val(globals, x, y))
        }
        (Val::Ctor(ea, ia, aa), Val::Ctor(eb, ib, ab)) => {
            ea == eb
                && ia == ib
                && aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab.iter())
                    .all(|(x, y)| same_val(globals, x, y))
        }
        (Val::Eq(ta, la, ra), Val::Eq(tb, lb, rb)) => {
            same_val(globals, ta, tb) && same_val(globals, la, lb) && same_val(globals, ra, rb)
        }
        (Val::Refl(x), Val::Refl(y)) => same_val(globals, x, y),
        (Val::Neutral(x), Val::Neutral(y)) => same_neutral(globals, x, y),
        _ => false,
    }
}

fn same_neutral(globals: &Globals, a: &Neutral, b: &Neutral) -> bool {
    match (a, b) {
        (Neutral::Var(i), Neutral::Var(j)) => i == j,
        (Neutral::Global(x), Neutral::Global(y)) => x == y,
        (Neutral::App(f, x), Neutral::App(g, y)) => {
            same_neutral(globals, f, g) && same_val(globals, x, y)
        }
        (
            Neutral::Match {
                env: ea,
                motive: ma,
                scrut: sa,
                branches: ba,
            },
            Neutral::Match {
                env: eb,
                motive: mb,
                scrut: sb,
                branches: bb,
            },
        ) => {
            if !same_val(globals, sa, sb) || ba.len() != bb.len() {
                return false;
            }
            let norm = 1_000_000 + ea.len().max(eb.len());
            let ma_n = crate::quote::quote_open(globals, norm, ea, ma);
            let mb_n = crate::quote::quote_open(globals, norm, eb, mb);
            if !crate::core::same_tm(&ma_n, &mb_n) {
                return false;
            }
            ba.iter().zip(bb.iter()).all(|(x, y)| {
                x.ctor == y.ctor
                    && x.binders == y.binders
                    && crate::core::same_tm(
                        &crate::quote::quote_open_binders(globals, norm, ea, x.binders, &x.body),
                        &crate::quote::quote_open_binders(globals, norm, eb, y.binders, &y.body),
                    )
            })
        }
        (Neutral::J { proof: p, .. }, Neutral::J { proof: q, .. }) => same_val(globals, p, q),
        _ => false,
    }
}
