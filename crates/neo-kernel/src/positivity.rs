use crate::core::{RcTm, Tm};
use crate::env::Globals;
use neo_common::error::{Error, Result};

/// Strict positivity check for one constructor field type.
pub fn check_field(_globals: &Globals, enum_name: &str, tm: &RcTm) -> Result<()> {
    if bad_occurrence(enum_name, tm) {
        return Err(Error::new(format!(
            "`{}` is not strictly positive in this constructor field (nested inductives are not supported)",
            enum_name
        )));
    }
    Ok(())
}

/// Constructor result indices must not mention the enum being defined. This is
/// a conservative positivity rule: an occurrence in an index could otherwise
/// hide the enum in a negative position (for example `Bad(fn(x: Bad) -> T)`).
pub fn check_index(_globals: &Globals, enum_name: &str, tm: &RcTm) -> Result<()> {
    if contains(enum_name, tm) {
        return Err(Error::new(format!(
            "`{}` occurs in a constructor result index; enum indices may not mention the enum being defined",
            enum_name
        )));
    }
    Ok(())
}

fn contains(enum_name: &str, tm: &RcTm) -> bool {
    match &**tm {
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop => false,
        Tm::Pi { dom, cod, .. } => contains(enum_name, dom) || contains(enum_name, cod),
        Tm::Lam { dom, body, .. } => contains(enum_name, dom) || contains(enum_name, body),
        Tm::App(f, a) => contains(enum_name, f) || contains(enum_name, a),
        Tm::Ind(name, args) => name == enum_name || args.iter().any(|a| contains(enum_name, a)),
        Tm::Ctor(name, _, args) => name == enum_name || args.iter().any(|a| contains(enum_name, a)),
        Tm::Match {
            motive,
            scrut,
            branches,
        } => {
            contains(enum_name, motive)
                || contains(enum_name, scrut)
                || branches.iter().any(|b| contains(enum_name, &b.body))
        }
        Tm::Eq(a, b, c) => {
            contains(enum_name, a) || contains(enum_name, b) || contains(enum_name, c)
        }
        Tm::Refl(a) => contains(enum_name, a),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => {
            contains(enum_name, ty)
                || contains(enum_name, lhs)
                || contains(enum_name, rhs)
                || contains(enum_name, proof)
                || contains(enum_name, motive)
                || contains(enum_name, refl_case)
        }
    }
}

fn bad_occurrence(enum_name: &str, tm: &RcTm) -> bool {
    match &**tm {
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop => false,
        Tm::Pi { dom, cod, .. } => contains(enum_name, dom) || bad_occurrence(enum_name, cod),
        Tm::Lam { body, .. } => bad_occurrence(enum_name, body),
        // A direct occurrence, `Ind(enum, args)`, is positive as long as the
        // arguments do not mention the enum (nested inductives are rejected).
        Tm::Ind(name, args) => {
            let self_occurs = name == enum_name;
            args.iter().any(|a| contains(enum_name, a))
                || (!self_occurs && args.iter().any(|a| contains(enum_name, a)))
        }
        Tm::Ctor(name, _, args) => name == enum_name || args.iter().any(|a| contains(enum_name, a)),
        Tm::App(f, a) => {
            // Application in a field type must not hide an occurrence.
            contains(enum_name, f) || contains(enum_name, a)
        }
        Tm::Match {
            motive,
            scrut,
            branches,
        } => {
            contains(enum_name, motive)
                || contains(enum_name, scrut)
                || branches.iter().any(|b| contains(enum_name, &b.body))
        }
        Tm::Eq(a, b, c) => {
            contains(enum_name, a) || contains(enum_name, b) || contains(enum_name, c)
        }
        Tm::Refl(a) => contains(enum_name, a),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => {
            contains(enum_name, ty)
                || contains(enum_name, lhs)
                || contains(enum_name, rhs)
                || contains(enum_name, proof)
                || contains(enum_name, motive)
                || contains(enum_name, refl_case)
        }
    }
}
