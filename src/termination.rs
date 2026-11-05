use std::collections::HashMap;

use crate::env::Globals;
use crate::error::{Error, Result};
use crate::surface::{Block, Expr, FnDecl, Pattern};

/// Check that every recursive call is structurally decreasing in one fixed
/// argument position. This is deliberately conservative.
pub fn check_fn(_globals: &Globals, decl: &FnDecl) -> Result<()> {
    let fname = decl.name.as_str();
    let mut sizes: HashMap<String, usize> = HashMap::new();
    for p in &decl.params {
        sizes.insert(p.name.clone(), 0);
    }
    let mut calls: Vec<Call> = Vec::new();
    walk(
        &Expr::Block(decl.body.clone()),
        &mut sizes,
        fname,
        &mut calls,
    );
    if calls.is_empty() {
        return Ok(());
    }

    let arity = decl.params.len();
    let mut common: Option<Vec<usize>> = None;
    for call in &calls {
        if call.arity != arity {
            return Err(Error::at(
                call.span,
                format!(
                    "recursive call to `{}` has {} argument(s), but it takes {}",
                    fname, call.arity, arity
                ),
            ));
        }
        if let Some(span) = call.bad_nonvar {
            return Err(Error::at(
                span,
                "recursive call uses a non-variable expression built from a structurally smaller value",
            ));
        }
        let smaller = &call.smaller;
        if smaller.is_empty() {
            return Err(Error::at(
                call.span,
                format!(
                    "recursive call to `{}` is not structurally decreasing; no argument is a subterm of a parameter",
                    fname
                ),
            ));
        }
        common = Some(match common {
            None => smaller.clone(),
            Some(prev) => prev.into_iter().filter(|i| smaller.contains(i)).collect(),
        });
        if common.as_ref().is_some_and(|v| v.is_empty()) {
            return Err(Error::at(
                call.span,
                "recursive calls do not decrease in a single consistent argument position",
            ));
        }
    }
    Ok(())
}

struct Call {
    span: crate::error::Span,
    arity: usize,
    smaller: Vec<usize>,
    bad_nonvar: Option<crate::error::Span>,
}

fn is_smaller(expr: &Expr, sizes: &HashMap<String, usize>) -> bool {
    match expr {
        Expr::Var(n, _) => sizes.get(n).is_some_and(|s| *s > 0 && *s != usize::MAX),
        _ => false,
    }
}

fn contains_smaller(expr: &Expr, sizes: &HashMap<String, usize>) -> bool {
    match expr {
        Expr::Var(n, _) => sizes.get(n).is_some_and(|s| *s > 0),
        Expr::Paren(e, _) => contains_smaller(e, sizes),
        Expr::App(f, args, _) => {
            contains_smaller(f, sizes) || args.iter().any(|a| contains_smaller(a, sizes))
        }
        Expr::FnType(binders, body, _) => {
            binders.iter().any(|b| contains_smaller(&b.ty, sizes)) || contains_smaller(body, sizes)
        }
        Expr::FnLam(binders, body, _) => {
            binders.iter().any(|b| contains_smaller(&b.ty, sizes)) || contains_smaller(body, sizes)
        }
        Expr::Match(s, arms, _) => {
            contains_smaller(s, sizes)
                || arms.iter().any(|a| {
                    pattern_contains_smaller(&a.pattern, sizes) || contains_smaller(&a.body, sizes)
                })
        }
        Expr::Eq(a, b, _) => contains_smaller(a, sizes) || contains_smaller(b, sizes),
        Expr::Block(b) => {
            b.bindings.iter().any(|x| contains_smaller(&x.value, sizes))
                || contains_smaller(&b.tail, sizes)
        }
        Expr::Nat(_, _) | Expr::Type(_, _) | Expr::Prop(_) | Expr::Refl(_) => false,
    }
}

fn pattern_contains_smaller(_p: &Pattern, _sizes: &HashMap<String, usize>) -> bool {
    false
}

fn walk(expr: &Expr, sizes: &mut HashMap<String, usize>, fname: &str, calls: &mut Vec<Call>) {
    match expr {
        Expr::Var(_, _) | Expr::Nat(_, _) | Expr::Type(_, _) | Expr::Prop(_) | Expr::Refl(_) => {}
        Expr::Paren(e, _) => walk(e, sizes, fname, calls),
        Expr::App(f, args, span) => {
            if let Expr::Var(n, _) = &**f
                && n == fname
            {
                let mut smaller = Vec::new();
                let mut bad_nonvar = None;
                for (i, a) in args.iter().enumerate() {
                    match a {
                        Expr::Var(_, _) if is_smaller(a, sizes) => smaller.push(i),
                        Expr::Var(_, _) => {}
                        other => {
                            if bad_nonvar.is_none() && contains_smaller(other, sizes) {
                                bad_nonvar = Some(other.span());
                            }
                        }
                    }
                }
                calls.push(Call {
                    span: *span,
                    arity: args.len(),
                    smaller,
                    bad_nonvar,
                });
            }
            walk(f, sizes, fname, calls);
            for a in args {
                walk(a, sizes, fname, calls);
            }
        }
        Expr::FnType(binders, body, _) => {
            for b in binders {
                walk(&b.ty, sizes, fname, calls);
            }
            walk(body, sizes, fname, calls);
        }
        Expr::FnLam(binders, body, _) => {
            let mut saved = Vec::new();
            for b in binders {
                walk(&b.ty, sizes, fname, calls);
                if let Some(n) = &b.name {
                    saved.push((n.clone(), sizes.insert(n.clone(), usize::MAX)));
                }
            }
            walk(body, sizes, fname, calls);
            restore(sizes, saved);
        }
        Expr::Match(scrut, arms, _) => {
            walk(scrut, sizes, fname, calls);
            let base = match &**scrut {
                Expr::Var(n, _) => sizes.get(n).copied(),
                _ => None,
            };
            for arm in arms {
                let mut saved = Vec::new();
                match &arm.pattern {
                    Pattern::Ctor(_, ps, _) => {
                        let s = base.map_or(usize::MAX, |s| s.saturating_add(1));
                        for p in ps {
                            if let Pattern::Var(n, _) = p {
                                saved.push((n.clone(), sizes.insert(n.clone(), s)));
                            }
                        }
                    }
                    Pattern::Var(n, _) => {
                        saved.push((
                            n.clone(),
                            sizes.insert(n.clone(), base.unwrap_or(usize::MAX)),
                        ));
                    }
                    Pattern::Wild(_) => {}
                }
                walk(&arm.body, sizes, fname, calls);
                restore(sizes, saved);
            }
        }
        Expr::Eq(a, b, _) => {
            walk(a, sizes, fname, calls);
            walk(b, sizes, fname, calls);
        }
        Expr::Block(b) => walk_block(b, sizes, fname, calls),
    }
}

fn walk_block(
    block: &Block,
    sizes: &mut HashMap<String, usize>,
    fname: &str,
    calls: &mut Vec<Call>,
) {
    let mut saved = Vec::new();
    for b in &block.bindings {
        walk(&b.value, sizes, fname, calls);
        let s = match &b.value {
            Expr::Var(v, _) => sizes.get(v).copied().unwrap_or(usize::MAX),
            _ => usize::MAX,
        };
        saved.push((b.name.clone(), sizes.insert(b.name.clone(), s)));
    }
    walk(&block.tail, sizes, fname, calls);
    restore(sizes, saved);
}

fn restore(sizes: &mut HashMap<String, usize>, saved: Vec<(String, Option<usize>)>) {
    for (name, old) in saved.into_iter().rev() {
        match old {
            Some(v) => {
                sizes.insert(name, v);
            }
            None => {
                sizes.remove(&name);
            }
        }
    }
}
