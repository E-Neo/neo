use std::rc::Rc;

pub type RcTm = Rc<Tm>;

/// A primitive type. Primitives are opaque: they have no constructors and are
/// eliminated only by the primitive operations below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimTy {
    U32,
}

impl PrimTy {
    pub fn name(self) -> &'static str {
        match self {
            PrimTy::U32 => "U32",
        }
    }
}

/// The result of evaluating a primitive operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimOutcome {
    U32(u32),
    Bool(bool),
}

/// A primitive operation. Operations are binary and must be fully applied;
/// they are not first-class values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl PrimOp {
    pub const ALL: [PrimOp; 16] = [
        PrimOp::Add,
        PrimOp::Sub,
        PrimOp::Mul,
        PrimOp::Div,
        PrimOp::Rem,
        PrimOp::And,
        PrimOp::Or,
        PrimOp::Xor,
        PrimOp::Shl,
        PrimOp::Shr,
        PrimOp::Lt,
        PrimOp::Le,
        PrimOp::Gt,
        PrimOp::Ge,
        PrimOp::Eq,
        PrimOp::Ne,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PrimOp::Add => "u32_add",
            PrimOp::Sub => "u32_sub",
            PrimOp::Mul => "u32_mul",
            PrimOp::Div => "u32_div",
            PrimOp::Rem => "u32_rem",
            PrimOp::And => "u32_and",
            PrimOp::Or => "u32_or",
            PrimOp::Xor => "u32_xor",
            PrimOp::Shl => "u32_shl",
            PrimOp::Shr => "u32_shr",
            PrimOp::Lt => "u32_lt",
            PrimOp::Le => "u32_le",
            PrimOp::Gt => "u32_gt",
            PrimOp::Ge => "u32_ge",
            PrimOp::Eq => "u32_eq",
            PrimOp::Ne => "u32_ne",
        }
    }

    pub fn from_name(name: &str) -> Option<PrimOp> {
        PrimOp::ALL.iter().copied().find(|op| op.name() == name)
    }

    /// Whether this operation produces a `Bool` instead of a `U32`.
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            PrimOp::Lt | PrimOp::Le | PrimOp::Gt | PrimOp::Ge | PrimOp::Eq | PrimOp::Ne
        )
    }

    /// Execute the operation on two machine words.
    ///
    /// Arithmetic wraps, division and remainder by zero return zero, and shift
    /// amounts use their low five bits. This matches the WebAssembly `i32`
    /// semantics that the backend emits.
    pub fn apply(self, a: u32, b: u32) -> PrimOutcome {
        use PrimOutcome::{Bool, U32};
        match self {
            PrimOp::Add => U32(a.wrapping_add(b)),
            PrimOp::Sub => U32(a.wrapping_sub(b)),
            PrimOp::Mul => U32(a.wrapping_mul(b)),
            PrimOp::Div => U32(a.checked_div(b).unwrap_or(0)),
            PrimOp::Rem => U32(a.checked_rem(b).unwrap_or(0)),
            PrimOp::And => U32(a & b),
            PrimOp::Or => U32(a | b),
            PrimOp::Xor => U32(a ^ b),
            PrimOp::Shl => U32(a << (b & 31)),
            PrimOp::Shr => U32(a >> (b & 31)),
            PrimOp::Lt => Bool(a < b),
            PrimOp::Le => Bool(a <= b),
            PrimOp::Gt => Bool(a > b),
            PrimOp::Ge => Bool(a >= b),
            PrimOp::Eq => Bool(a == b),
            PrimOp::Ne => Bool(a != b),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Tm {
    Var(usize),
    Const(String),
    Type(u32),
    Prop,
    /// A primitive type former.
    PrimTy(PrimTy),
    /// A machine-word literal.
    PrimLit(u32),
    /// A fully applied primitive operation.
    PrimOp(PrimOp, RcTm, RcTm),
    Pi {
        name: String,
        dom: RcTm,
        cod: RcTm,
        implicit: bool,
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
    /// True when the constructor's result indices cannot unify with the
    /// scrutinee type. Such a branch is unreachable for well-typed scrutinees:
    /// the kernel does not check its body, and the evaluator never selects it.
    pub impossible: bool,
}

pub fn rc(t: Tm) -> RcTm {
    Rc::new(t)
}

pub fn var(i: usize) -> RcTm {
    rc(Tm::Var(i))
}

pub fn prim_ty(t: PrimTy) -> RcTm {
    rc(Tm::PrimTy(t))
}

pub fn prim_lit(n: u32) -> RcTm {
    rc(Tm::PrimLit(n))
}

pub fn prim_op(op: PrimOp, a: RcTm, b: RcTm) -> RcTm {
    rc(Tm::PrimOp(op, a, b))
}

pub fn app(f: RcTm, a: RcTm) -> RcTm {
    rc(Tm::App(f, a))
}

pub fn app_many(f: RcTm, args: Vec<RcTm>) -> RcTm {
    args.into_iter().fold(f, app)
}

pub fn pi(name: impl Into<String>, dom: RcTm, cod: RcTm) -> RcTm {
    pi_with(name, dom, cod, false)
}

pub fn pi_with(name: impl Into<String>, dom: RcTm, cod: RcTm, implicit: bool) -> RcTm {
    rc(Tm::Pi {
        name: name.into(),
        dom,
        cod,
        implicit,
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
        Tm::Const(_) | Tm::Type(_) | Tm::Prop | Tm::PrimTy(_) | Tm::PrimLit(_) => tm.clone(),
        Tm::PrimOp(op, a, b) => prim_op(*op, shift(a, delta, cutoff), shift(b, delta, cutoff)),
        Tm::Pi {
            name,
            dom,
            cod,
            implicit,
        } => pi_with(
            name.clone(),
            shift(dom, delta, cutoff),
            shift(cod, delta, cutoff + 1),
            *implicit,
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
                    impossible: b.impossible,
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
        Tm::Const(_) | Tm::Type(_) | Tm::Prop | Tm::PrimTy(_) | Tm::PrimLit(_) => tm.clone(),
        Tm::PrimOp(op, a, b) => prim_op(
            *op,
            subst_at(a, index, replacement, depth),
            subst_at(b, index, replacement, depth),
        ),
        Tm::Pi {
            name,
            dom,
            cod,
            implicit,
        } => pi_with(
            name.clone(),
            subst_at(dom, index, replacement, depth),
            subst_at(cod, index, replacement, depth + 1),
            *implicit,
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
                    impossible: b.impossible,
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

/// Replace every occurrence of `target` in `tm` with `replacement`, shifting
/// under binders so that no free variable is captured. `target`, `replacement`,
/// and `tm` are terms in the same de Bruijn context.
pub fn abstract_term(target: &RcTm, replacement: &RcTm, tm: &RcTm) -> RcTm {
    abstract_at(target, replacement, tm, 0)
}

fn abstract_at(target: &RcTm, replacement: &RcTm, tm: &RcTm, depth: usize) -> RcTm {
    let target_here = shift(target, depth as isize, 0);
    if same_tm(tm, &target_here) {
        return shift(replacement, depth as isize, 0);
    }
    match &**tm {
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop | Tm::PrimTy(_) | Tm::PrimLit(_) => {
            tm.clone()
        }
        Tm::PrimOp(op, a, b) => prim_op(
            *op,
            abstract_at(target, replacement, a, depth),
            abstract_at(target, replacement, b, depth),
        ),
        Tm::Pi {
            name,
            dom,
            cod,
            implicit,
        } => pi_with(
            name.clone(),
            abstract_at(target, replacement, dom, depth),
            abstract_at(target, replacement, cod, depth + 1),
            *implicit,
        ),
        Tm::Lam { name, dom, body } => lam(
            name.clone(),
            abstract_at(target, replacement, dom, depth),
            abstract_at(target, replacement, body, depth + 1),
        ),
        Tm::App(f, a) => app(
            abstract_at(target, replacement, f, depth),
            abstract_at(target, replacement, a, depth),
        ),
        Tm::Ind(name, args) => ind(
            name.clone(),
            args.iter()
                .map(|a| abstract_at(target, replacement, a, depth))
                .collect(),
        ),
        Tm::Ctor(name, index, args) => ctor(
            name.clone(),
            *index,
            args.iter()
                .map(|a| abstract_at(target, replacement, a, depth))
                .collect(),
        ),
        Tm::Match {
            motive,
            scrut,
            branches,
        } => rc(Tm::Match {
            motive: abstract_at(target, replacement, motive, depth),
            scrut: abstract_at(target, replacement, scrut, depth),
            branches: branches
                .iter()
                .map(|b| Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: abstract_at(target, replacement, &b.body, depth + b.binders),
                    impossible: b.impossible,
                })
                .collect(),
        }),
        Tm::Eq(a, b, c) => eq(
            abstract_at(target, replacement, a, depth),
            abstract_at(target, replacement, b, depth),
            abstract_at(target, replacement, c, depth),
        ),
        Tm::Refl(a) => rc(Tm::Refl(abstract_at(target, replacement, a, depth))),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => rc(Tm::J {
            ty: abstract_at(target, replacement, ty, depth),
            lhs: abstract_at(target, replacement, lhs, depth),
            rhs: abstract_at(target, replacement, rhs, depth),
            proof: abstract_at(target, replacement, proof, depth),
            motive: abstract_at(target, replacement, motive, depth),
            refl_case: abstract_at(target, replacement, refl_case, depth),
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
        (Tm::PrimTy(a), Tm::PrimTy(b)) => a == b,
        (Tm::PrimLit(a), Tm::PrimLit(b)) => a == b,
        (Tm::PrimOp(o1, a1, b1), Tm::PrimOp(o2, a2, b2)) => {
            o1 == o2 && same_tm(a1, a2) && same_tm(b1, b2)
        }
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
                    x.ctor == y.ctor
                        && x.binders == y.binders
                        && x.impossible == y.impossible
                        && same_tm(&x.body, &y.body)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn konst(s: &str) -> RcTm {
        rc(Tm::Const(s.to_string()))
    }

    #[test]
    fn abstract_term_avoids_capture() {
        // Bind `x` and use the outer `Var(1)`; the occurrence under the
        // binder shifts to `Var(2)` and the replacement shifts to `Var(1)`.
        let target = var(1);
        let replacement = var(0);
        let tm = lam("x", rc(Tm::Type(0)), app(var(0), var(2)));
        let got = abstract_term(&target, &replacement, &tm);
        let expected = lam("x", rc(Tm::Type(0)), app(var(0), var(1)));
        assert!(same_tm(&got, &expected));
    }

    #[test]
    fn abstract_term_replaces_under_binders() {
        let target = konst("t");
        let replacement = var(0);
        // `fn(x: Type 0) -> t` with `t` replaced by the outer `Var(0)`.
        let tm = pi("x", rc(Tm::Type(0)), target.clone());
        let got = abstract_term(&target, &replacement, &tm);
        let expected = pi("x", rc(Tm::Type(0)), var(1));
        assert!(same_tm(&got, &expected));
    }

    #[test]
    fn abstract_term_leaves_other_variables_alone() {
        let target = konst("c");
        let replacement = konst("c");
        let tm = lam("x", rc(Tm::Type(0)), app(var(0), konst("t")));
        assert!(same_tm(&abstract_term(&target, &replacement, &tm), &tm));
    }
}
