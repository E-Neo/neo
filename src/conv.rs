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
            if !conv(globals, level, da, db) {
                return false;
            }
            let fresh = vvar(level);
            let ca = eval_closure(globals, ca, fresh.clone());
            let cb = eval_closure(globals, cb, fresh);
            conv(globals, level + 1, &ca, &cb)
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
        (Val::Glob(a), Val::Glob(b)) => a == b,
        (Val::Glob(a), Val::Neutral(n)) | (Val::Neutral(n), Val::Glob(a)) => {
            matches!(&**n, Neutral::Global(b) if a == b)
        }
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
            if !conv(globals, level, da, db) {
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
            if ba.len() != bb.len() {
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
                if x.ctor != y.ctor || x.binders != y.binders || x.impossible != y.impossible {
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
                env: ea,
                ty: ta,
                lhs: la,
                rhs: ra,
                proof: pa,
                motive: rta,
                refl_case: rca,
            },
            Neutral::J {
                env: eb,
                ty: tb,
                lhs: lb,
                rhs: rb,
                proof: pb,
                motive: rtb,
                refl_case: rcb,
            },
        ) => {
            let norm = 1_000_000 + ea.len().max(eb.len());
            let open = |env, tm| crate::quote::quote_open(globals, norm, env, tm);
            conv(globals, level, pa, pb)
                && crate::core::same_tm(&open(ea, ta), &open(eb, tb))
                && crate::core::same_tm(&open(ea, la), &open(eb, lb))
                && crate::core::same_tm(&open(ea, ra), &open(eb, rb))
                && crate::core::same_tm(&open(ea, rta), &open(eb, rtb))
                && crate::core::same_tm(&open(ea, rca), &open(eb, rcb))
        }
        _ => false,
    }
}
