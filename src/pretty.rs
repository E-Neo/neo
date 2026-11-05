use crate::core::{RcTm, Tm};
use crate::env::Globals;
use crate::quote::quote;
use crate::value::V;

pub fn show_val(globals: &Globals, level: usize, v: &V) -> String {
    let tm = quote(globals, level, v);
    let mut names: Vec<String> = (0..level).map(|i| format!("x{}", i)).collect();
    term_at(globals, &mut names, &tm)
}

pub fn show_term(globals: &Globals, tm: &RcTm) -> String {
    let mut names = Vec::new();
    term_at(globals, &mut names, tm)
}

fn term_at(globals: &Globals, names: &mut Vec<String>, tm: &RcTm) -> String {
    match &**tm {
        Tm::Var(i) => {
            if *i < names.len() {
                names[names.len() - 1 - i].clone()
            } else {
                format!("#{}", i)
            }
        }
        Tm::Const(name) => name.clone(),
        Tm::Type(0) => "Type 0".to_string(),
        Tm::Type(u) => format!("Type {}", u),
        Tm::Prop => "Prop".to_string(),
        Tm::Pi { name, dom, cod } => {
            let d = term_at(globals, names, dom);
            names.push(name.clone());
            let c = term_at(globals, names, cod);
            names.pop();
            format!("fn({}: {}) -> {}", name, d, c)
        }
        Tm::Lam { name, dom, body } => {
            let d = term_at(globals, names, dom);
            names.push(name.clone());
            let b = term_at(globals, names, body);
            names.pop();
            format!("fn({}: {}) => {}", name, d, b)
        }
        Tm::App(_, _) => {
            let (head, args) = flatten_app(tm);
            let h = term_at(globals, names, &head);
            let a: Vec<String> = args.iter().map(|x| term_at(globals, names, x)).collect();
            format!("{}({})", h, a.join(", "))
        }
        Tm::Ind(name, args) => {
            if args.is_empty() {
                name.clone()
            } else {
                let a: Vec<String> = args.iter().map(|x| term_at(globals, names, x)).collect();
                format!("{}({})", name, a.join(", "))
            }
        }
        Tm::Ctor(enum_name, index, args) => {
            if let Some(n) = nat_value(globals, enum_name, *index, args) {
                return n.to_string();
            }
            let cname = globals
                .get_ctor_info(enum_name, *index)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| format!("{}#{}", enum_name, index));
            if args.is_empty() {
                cname
            } else {
                let a: Vec<String> = args.iter().map(|x| term_at(globals, names, x)).collect();
                format!("{}({})", cname, a.join(", "))
            }
        }
        Tm::Match {
            motive,
            scrut,
            branches,
        } => {
            let m = term_at(globals, names, motive);
            let s = term_at(globals, names, scrut);
            let mut out = format!("match {} {{", s);
            for b in branches {
                out.push_str(&format!("\n  {}(", b.ctor));
                for i in 0..b.binders {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&format!("a{}", i));
                }
                out.push_str(") => ");
                for i in 0..b.binders {
                    names.push(format!("a{}", i));
                }
                out.push_str(&term_at(globals, names, &b.body));
                for _ in 0..b.binders {
                    names.pop();
                }
            }
            out.push_str(&format!("\n  // motive: {}\n}}", m));
            out
        }
        Tm::Eq(_, b, c) => format!(
            "{} == {}",
            term_at(globals, names, b),
            term_at(globals, names, c)
        ),
        Tm::Refl(a) => format!("refl({})", term_at(globals, names, a)),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            result_ty,
            refl_case,
        } => format!(
            "J({}, {}, {}, {}, {}, {})",
            term_at(globals, names, ty),
            term_at(globals, names, lhs),
            term_at(globals, names, rhs),
            term_at(globals, names, proof),
            term_at(globals, names, result_ty),
            term_at(globals, names, refl_case)
        ),
    }
}

fn flatten_app(tm: &RcTm) -> (RcTm, Vec<RcTm>) {
    let mut args = Vec::new();
    let mut cur = tm.clone();
    while let Tm::App(f, a) = &*cur.clone() {
        args.push(a.clone());
        cur = f.clone();
    }
    args.reverse();
    (cur, args)
}

pub fn nat_value(globals: &Globals, enum_name: &str, index: usize, args: &[RcTm]) -> Option<u64> {
    if enum_name != "Nat" {
        return None;
    }
    let info = globals.get_enum("Nat")?;
    let zero = info.ctors.iter().position(|c| c.name == "zero")?;
    let succ = info.ctors.iter().position(|c| c.name == "succ")?;
    if index == zero && args.is_empty() {
        Some(0)
    } else if index == succ && args.len() == 1 {
        match &*args[0] {
            Tm::Ctor(name, i, a) => Some(1 + nat_value(globals, name, *i, a)?),
            _ => None,
        }
    } else {
        None
    }
}
