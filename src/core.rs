use std::rc::Rc;

pub type RcTm = Rc<Tm>;

#[derive(Clone, Debug)]
pub enum Tm {
    Var(usize),
    Const(String),
    Type(u32),
    Prop,
    Pi {
        name: String,
        dom: RcTm,
        cod: RcTm,
    },
    Lam {
        name: String,
        dom: RcTm,
        body: RcTm,
    },
    App(RcTm, RcTm),
    Ind(String, Vec<RcTm>),
    Ctor(String, usize, Vec<RcTm>),
    Match {
        motive: RcTm,
        scrut: RcTm,
        branches: Vec<Branch>,
    },
    Eq(RcTm, RcTm, RcTm),
    Refl(RcTm),
    J {
        ty: RcTm,
        lhs: RcTm,
        rhs: RcTm,
        proof: RcTm,
        motive: RcTm,
        refl_case: RcTm,
    },
}

#[derive(Clone, Debug)]
pub struct Branch {
    pub ctor: String,
    pub binders: usize,
    pub body: RcTm,
}

pub fn rc(t: Tm) -> RcTm {
    Rc::new(t)
}

pub fn var(i: usize) -> RcTm {
    rc(Tm::Var(i))
}

pub fn app(f: RcTm, a: RcTm) -> RcTm {
    rc(Tm::App(f, a))
}

pub fn app_many(f: RcTm, args: Vec<RcTm>) -> RcTm {
    args.into_iter().fold(f, app)
}

pub fn pi(name: impl Into<String>, dom: RcTm, cod: RcTm) -> RcTm {
    rc(Tm::Pi {
        name: name.into(),
        dom,
        cod,
    })
}

pub fn lam(name: impl Into<String>, dom: RcTm, body: RcTm) -> RcTm {
    rc(Tm::Lam {
        name: name.into(),
        dom,
        body,
    })
}

pub fn const_(name: impl Into<String>) -> RcTm {
    rc(Tm::Const(name.into()))
}

pub fn ind(name: impl Into<String>, args: Vec<RcTm>) -> RcTm {
    rc(Tm::Ind(name.into(), args))
}

pub fn ctor(enum_name: impl Into<String>, index: usize, args: Vec<RcTm>) -> RcTm {
    rc(Tm::Ctor(enum_name.into(), index, args))
}

pub fn eq(ty: RcTm, lhs: RcTm, rhs: RcTm) -> RcTm {
    rc(Tm::Eq(ty, lhs, rhs))
}

/// Shift free de Bruijn indices by `delta`, respecting binders.
pub fn shift(tm: &RcTm, delta: isize, cutoff: usize) -> RcTm {
    match &**tm {
        Tm::Var(i) => {
            if *i >= cutoff {
                var((*i as isize + delta) as usize)
            } else {
                tm.clone()
            }
        }
        Tm::Const(_) | Tm::Type(_) | Tm::Prop => tm.clone(),
        Tm::Pi { name, dom, cod } => pi(
            name.clone(),
            shift(dom, delta, cutoff),
            shift(cod, delta, cutoff + 1),
        ),
        Tm::Lam { name, dom, body } => lam(
            name.clone(),
            shift(dom, delta, cutoff),
            shift(body, delta, cutoff + 1),
        ),
        Tm::App(f, a) => app(shift(f, delta, cutoff), shift(a, delta, cutoff)),
        Tm::Ind(name, args) => ind(
            name.clone(),
            args.iter().map(|a| shift(a, delta, cutoff)).collect(),
        ),
        Tm::Ctor(name, index, args) => ctor(
            name.clone(),
            *index,
            args.iter().map(|a| shift(a, delta, cutoff)).collect(),
        ),
        Tm::Match {
            motive,
            scrut,
            branches,
        } => rc(Tm::Match {
            motive: shift(motive, delta, cutoff),
            scrut: shift(scrut, delta, cutoff),
            branches: branches
                .iter()
                .map(|b| Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: shift(&b.body, delta, cutoff + b.binders),
                })
                .collect(),
        }),
        Tm::Eq(a, b, c) => eq(
            shift(a, delta, cutoff),
            shift(b, delta, cutoff),
            shift(c, delta, cutoff),
        ),
        Tm::Refl(a) => rc(Tm::Refl(shift(a, delta, cutoff))),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => rc(Tm::J {
            ty: shift(ty, delta, cutoff),
            lhs: shift(lhs, delta, cutoff),
            rhs: shift(rhs, delta, cutoff),
            proof: shift(proof, delta, cutoff),
            motive: shift(motive, delta, cutoff),
            refl_case: shift(refl_case, delta, cutoff),
        }),
    }
}

/// Substitute `replacement` for `Var(index)` in `tm`, shifting the
/// replacement under binders as required. The result is a capture-avoiding
/// substitution in the de Bruijn representation.
pub fn subst(tm: &RcTm, index: usize, replacement: &RcTm) -> RcTm {
    subst_at(tm, index, replacement, 0)
}

fn subst_at(tm: &RcTm, index: usize, replacement: &RcTm, depth: usize) -> RcTm {
    match &**tm {
        Tm::Var(i) => {
            if *i == index + depth {
                shift(replacement, depth as isize, 0)
            } else {
                tm.clone()
            }
        }
        Tm::Const(_) | Tm::Type(_) | Tm::Prop => tm.clone(),
        Tm::Pi { name, dom, cod } => pi(
            name.clone(),
            subst_at(dom, index, replacement, depth),
            subst_at(cod, index, replacement, depth + 1),
        ),
        Tm::Lam { name, dom, body } => lam(
            name.clone(),
            subst_at(dom, index, replacement, depth),
            subst_at(body, index, replacement, depth + 1),
        ),
        Tm::App(f, a) => app(
            subst_at(f, index, replacement, depth),
            subst_at(a, index, replacement, depth),
        ),
        Tm::Ind(name, args) => ind(
            name.clone(),
            args.iter()
                .map(|a| subst_at(a, index, replacement, depth))
                .collect(),
        ),
        Tm::Ctor(name, i, args) => ctor(
            name.clone(),
            *i,
            args.iter()
                .map(|a| subst_at(a, index, replacement, depth))
                .collect(),
        ),
        Tm::Match {
            motive,
            scrut,
            branches,
        } => rc(Tm::Match {
            motive: subst_at(motive, index, replacement, depth),
            scrut: subst_at(scrut, index, replacement, depth),
            branches: branches
                .iter()
                .map(|b| Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: subst_at(&b.body, index, replacement, depth + b.binders),
                })
                .collect(),
        }),
        Tm::Eq(a, b, c) => eq(
            subst_at(a, index, replacement, depth),
            subst_at(b, index, replacement, depth),
            subst_at(c, index, replacement, depth),
        ),
        Tm::Refl(a) => rc(Tm::Refl(subst_at(a, index, replacement, depth))),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => rc(Tm::J {
            ty: subst_at(ty, index, replacement, depth),
            lhs: subst_at(lhs, index, replacement, depth),
            rhs: subst_at(rhs, index, replacement, depth),
            proof: subst_at(proof, index, replacement, depth),
            motive: subst_at(motive, index, replacement, depth),
            refl_case: subst_at(refl_case, index, replacement, depth),
        }),
    }
}

/// Structural equality of terms up to binder names. This is a sound
/// approximation of conversion used for neutral eliminators.
pub fn same_tm(a: &RcTm, b: &RcTm) -> bool {
    match (&**a, &**b) {
        (Tm::Var(i), Tm::Var(j)) => i == j,
        (Tm::Const(x), Tm::Const(y)) => x == y,
        (Tm::Type(u), Tm::Type(v)) => u == v,
        (Tm::Prop, Tm::Prop) => true,
        (
            Tm::Pi {
                dom: d1, cod: c1, ..
            },
            Tm::Pi {
                dom: d2, cod: c2, ..
            },
        ) => same_tm(d1, d2) && same_tm(c1, c2),
        (
            Tm::Lam {
                dom: d1, body: b1, ..
            },
            Tm::Lam {
                dom: d2, body: b2, ..
            },
        ) => same_tm(d1, d2) && same_tm(b1, b2),
        (Tm::App(f1, a1), Tm::App(f2, a2)) => same_tm(f1, f2) && same_tm(a1, a2),
        (Tm::Ind(n1, a1), Tm::Ind(n2, a2)) => {
            n1 == n2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| same_tm(x, y))
        }
        (Tm::Ctor(e1, i1, a1), Tm::Ctor(e2, i2, a2)) => {
            e1 == e2
                && i1 == i2
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(x, y)| same_tm(x, y))
        }
        (
            Tm::Match {
                motive: m1,
                scrut: s1,
                branches: b1,
            },
            Tm::Match {
                motive: m2,
                scrut: s2,
                branches: b2,
            },
        ) => {
            same_tm(m1, m2)
                && same_tm(s1, s2)
                && b1.len() == b2.len()
                && b1.iter().zip(b2).all(|(x, y)| {
                    x.ctor == y.ctor && x.binders == y.binders && same_tm(&x.body, &y.body)
                })
        }
        (Tm::Eq(a1, b1, c1), Tm::Eq(a2, b2, c2)) => {
            same_tm(a1, a2) && same_tm(b1, b2) && same_tm(c1, c2)
        }
        (Tm::Refl(a1), Tm::Refl(a2)) => same_tm(a1, a2),
        (
            Tm::J {
                ty: t1,
                lhs: l1,
                rhs: r1,
                proof: p1,
                motive: m1,
                refl_case: rc1,
            },
            Tm::J {
                ty: t2,
                lhs: l2,
                rhs: r2,
                proof: p2,
                motive: m2,
                refl_case: rc2,
            },
        ) => {
            same_tm(t1, t2)
                && same_tm(l1, l2)
                && same_tm(r1, r2)
                && same_tm(p1, p2)
                && same_tm(m1, m2)
                && same_tm(rc1, rc2)
        }
        _ => false,
    }
}
