#![cfg(not(target_family = "wasm"))]

use neo::core::PrimLit;
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::error::Error;
use neo::eval::{eval, force};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::value::{V, Val};

fn elaborate(src: &str) -> Globals {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).expect("elaborate");
    globals
}

fn rejection(src: &str) -> Error {
    let mut globals = new_globals().expect("prelude");
    match parse_source(src) {
        Err(e) => e,
        Ok(ast) => elaborate_program(&mut globals, &ast).unwrap_err(),
    }
}

fn assert_rejected(src: &str, needle: &str) {
    let err = rejection(src);
    assert!(
        err.message.contains(needle),
        "expected error containing {:?}, got {:?}",
        needle,
        err.message
    );
}

fn main_value(globals: &Globals) -> V {
    let mut current = force(
        globals,
        eval(globals, &Vec::new(), &neo::core::const_("main")),
    );
    loop {
        let name = match &*current {
            Val::Glob(name) => name.clone(),
            _ => return current,
        };
        let next = force(globals, current);
        if matches!(&*next, Val::Glob(n) if *n == name) {
            return next;
        }
        current = next;
    }
}

fn as_nat(globals: &Globals, v: &V) -> u64 {
    let info = globals.get_enum("Nat").unwrap();
    let zero = info.ctors.iter().position(|c| c.name == "zero").unwrap();
    let succ = info.ctors.iter().position(|c| c.name == "succ").unwrap();
    fn go(v: &V, zero: usize, succ: usize) -> u64 {
        match &**v {
            Val::Ctor(name, i, _) if name == "Nat" && *i == zero => 0,
            Val::Ctor(name, i, args) if name == "Nat" && *i == succ => 1 + go(&args[0], zero, succ),
            other => panic!("not a nat: {other:?}"),
        }
    }
    go(v, zero, succ)
}

fn nat_main(body: &str) -> u64 {
    let globals = elaborate(&format!("fn main(): Nat {{\n{body}\n}}"));
    as_nat(&globals, &main_value(&globals))
}

fn u32_main(body: &str) -> u32 {
    let globals = elaborate(&format!("fn main(): U32 {{\n{body}\n}}"));
    match &*main_value(&globals) {
        Val::PrimLit(PrimLit::U32(n)) => *n,
        other => panic!("`main` did not evaluate to a u32 literal: {other:?}"),
    }
}

fn bool_main(body: &str) -> bool {
    let globals = elaborate(&format!("fn main(): Bool {{\n{body}\n}}"));
    match &*main_value(&globals) {
        Val::Ctor(name, index, _) if name == "Bool" => *index == 1,
        other => panic!("`main` did not evaluate to a bool: {other:?}"),
    }
}

#[test]
fn let_bindings() {
    assert_eq!(nat_main("let a: Nat = 2\n  let b = zero\n  a + b"), 2);
    assert_eq!(u32_main("let a = 3u32\n  a"), 3);
    assert_rejected(
        "fn main(): Nat {\n  let x = 1\n  x\n}",
        "cannot infer the type of this numeral",
    );
}

#[test]
fn let_bindings_shadow() {
    assert_eq!(nat_main("let a: Nat = 2\n  let a: Nat = a + 1\n  a"), 3);
    elaborate("fn main(): Nat {\n  let p: (zero = zero) = refl\n  0\n}");
}

#[test]
fn numerals_follow_the_expected_type() {
    assert_eq!(u32_main("42"), 42);
    assert_eq!(nat_main("42"), 42);
    assert_eq!(u32_main("1 + 2"), 3);
    assert_rejected(
        "fn main(): Nat { 1u32 }",
        "literal suffix `u32` conflicts with expected type `Nat`",
    );
    assert_rejected("fn main(): Bool { 1 }", "numeric literal cannot have type");
    assert_rejected("fn main(): Nat { 1000001 }", "too large");
}

#[test]
fn equality_split() {
    elaborate("fn refl_zero(): zero = zero {\n  refl\n}");
    assert!(bool_main("(1 : Nat) == 1"));
    assert!(bool_main("(1 : Nat) != 2"));
    assert!(bool_main("true != false"));
    assert!(bool_main("\"a\" != \"b\""));
    assert!(bool_main("unit == unit"));
    assert_rejected(
        "fn main(): Bool { 1 == 1 }",
        "cannot infer the operand type",
    );
    assert_rejected("fn main(): Nat { zero == zero }", "type mismatch");
    assert_rejected(
        "fn f(x: List[Nat]): Bool { x == x }",
        "boolean equality is not defined",
    );
}

#[test]
fn precedence_and_association() {
    assert_eq!(nat_main("(2 : Nat) + 3 * 4"), 14);
    assert_eq!(nat_main("((2 : Nat) + 3) * 4"), 20);
    assert_eq!(nat_main("(1 : Nat) << 2 + 1"), 8);
    assert_eq!(nat_main("(1 : Nat) | 2 & 3"), 3);
    assert_rejected(
        "fn main(): Bool { 1u32 < 2u32 < 3u32 }",
        "comparison operators are not associative",
    );
}

#[test]
fn nat_operator_semantics() {
    assert_eq!(nat_main("(5 : Nat) - 8"), 0);
    assert_eq!(nat_main("(7 : Nat) / 2"), 3);
    assert_eq!(nat_main("(7 : Nat) % 2"), 1);
    assert_eq!(nat_main("(7 : Nat) / 0"), 0);
    assert_eq!(nat_main("(12 : Nat) & 10"), 8);
    assert_eq!(nat_main("(12 : Nat) | 10"), 14);
    assert_eq!(nat_main("(12 : Nat) ^ 10"), 6);
    assert_eq!(nat_main("(1 : Nat) << 4"), 16);
    assert_eq!(nat_main("(32 : Nat) >> 3"), 4);
    assert!(bool_main("(1 : Nat) < 2"));
    assert!(bool_main("(2 : Nat) <= 2"));
    assert!(bool_main("(3 : Nat) > 2"));
    assert!(bool_main("(2 : Nat) >= 2"));
}

#[test]
fn u32_operator_semantics() {
    assert_eq!(u32_main("5 - 8"), 5u32.wrapping_sub(8));
    assert_eq!(u32_main("7 / 0"), 0);
    assert_eq!(u32_main("1 << 32"), 1);
    assert_eq!(u32_main("(12 & 10) + (12 | 10) + (12 ^ 10)"), 8 + 14 + 6);
    assert!(bool_main("1u32 < 2u32"));
    assert!(bool_main("2u32 <= 2u32"));
    assert!(bool_main("3u32 > 2u32"));
    assert!(bool_main("2u32 >= 2u32"));
}

#[test]
fn operators_use_the_operand_type() {
    assert_eq!(nat_main("1 + 2"), 3);
    assert_eq!(u32_main("1 + 2"), 3);
    assert_eq!(u32_main("1u32 + 2"), 3);
    assert_eq!(u32_main("1 + 2u32"), 3);
    assert_eq!(nat_main("let n: Nat = 1\n  n + 2"), 3);
    assert_eq!(u32_main("let n: U32 = 1\n  n + 2"), 3);
    assert_rejected(
        "fn main(): Nat {\n  let x = 1 + 2\n  x\n}",
        "cannot infer the numeric type",
    );
}

#[test]
fn ascription_directs_numerals() {
    assert_eq!(nat_main("(5 : Nat)"), 5);
    assert_eq!(u32_main("(5 : U32)"), 5);
    assert_rejected("fn main(): Nat { (5 : U32) }", "type mismatch");
}

#[test]
fn boolean_equality_is_typed_by_the_operands() {
    assert!(bool_main("(1 : Nat) == 1"));
    assert!(bool_main("1u32 == 1"));
    assert!(bool_main("true == true"));
    assert!(bool_main("\"abc\" == \"abc\""));
    assert!(bool_main("unit == unit"));
}
