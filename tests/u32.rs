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

fn u32_main(body: &str) -> u32 {
    let globals = elaborate(&format!("fn main(): U32 {{ {body} }}"));
    match &*main_value(&globals) {
        Val::PrimLit(PrimLit::U32(n)) => *n,
        other => panic!("`main` did not evaluate to a u32 literal: {other:?}"),
    }
}

fn bool_main(body: &str) -> bool {
    let globals = elaborate(&format!("fn main(): Bool {{ {body} }}"));
    match &*main_value(&globals) {
        Val::Ctor(name, index, _) if name == "Bool" => *index == 1,
        other => panic!("`main` did not evaluate to a bool: {other:?}"),
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

#[test]
fn arithmetic_wraps_and_division_is_total() {
    assert_eq!(u32_main("u32_add(20u32, 22u32)"), 42);
    assert_eq!(u32_main("u32_sub(0u32, 1u32)"), u32::MAX);
    assert_eq!(u32_main("u32_mul(65536u32, 65536u32)"), 0);
    assert_eq!(u32_main("u32_div(7u32, 0u32)"), 0);
    assert_eq!(u32_main("u32_rem(7u32, 0u32)"), 0);
    assert_eq!(u32_main("u32_div(7u32, 2u32)"), 3);
    assert_eq!(u32_main("u32_rem(7u32, 2u32)"), 1);
}

#[test]
fn bitwise_operations_use_machine_semantics() {
    assert_eq!(u32_main("u32_and(12u32, 10u32)"), 8);
    assert_eq!(u32_main("u32_or(12u32, 10u32)"), 14);
    assert_eq!(u32_main("u32_xor(12u32, 10u32)"), 6);
    assert_eq!(u32_main("u32_shl(1u32, 31u32)"), 1 << 31);
    assert_eq!(u32_main("u32_shr(2147483648u32, 31u32)"), 1);
    // Shift amounts use their low five bits, as in WebAssembly.
    assert_eq!(u32_main("u32_shl(1u32, 32u32)"), 1);
}

#[test]
fn comparisons_return_bool() {
    assert!(bool_main("u32_lt(1u32, 2u32)"));
    assert!(!bool_main("u32_lt(2u32, 1u32)"));
    assert!(bool_main("u32_le(2u32, 2u32)"));
    assert!(bool_main("u32_gt(3u32, 2u32)"));
    assert!(bool_main("u32_ge(2u32, 2u32)"));
    assert!(bool_main("u32_eq(2u32, 2u32)"));
    assert!(bool_main("u32_ne(2u32, 3u32)"));
}

#[test]
fn comparisons_reduce_during_elaboration() {
    let globals = elaborate(
        r#"
fn pick(): Nat {
  match u32_lt(1u32, 2u32) {
    true => 10
    false => 20
  }
}

fn main(): Nat {
  pick()
}
"#,
    );
    assert_eq!(as_nat(&globals, &main_value(&globals)), 10);
}

#[test]
fn u32_can_index_an_enum() {
    let globals = elaborate(
        r#"
enum Vec[T: Type, n: U32] {
  vnil : Vec[T, 0u32]
  vone(x: T) : Vec[T, 1u32]
}

fn first(v: Vec[Nat, 1u32]): Nat {
  match v {
    vone(x) => x
  }
}

fn main(): Nat {
  first(vone(7))
}
"#,
    );
    assert_eq!(as_nat(&globals, &main_value(&globals)), 7);
}

#[test]
fn accepts_plain_literals_where_u32_is_expected() {
    assert_eq!(u32_main("u32_add(1, 2)"), 3);
    assert_rejected(
        "fn main(): Nat { 1u32 }",
        "literal suffix `u32` conflicts with expected type `Nat`",
    );
}

#[test]
fn rejects_misapplied_primitives() {
    assert_rejected("fn main(): U32 { u32_add(1u32) }", "takes 2 arguments");
    assert_rejected(
        "fn main(): U32 { u32_add[1u32](1u32, 2u32) }",
        "takes no implicit arguments",
    );
    assert_rejected("fn main(): U32 { u32_add }", "must be fully applied");
}

#[test]
fn rejects_primitive_redeclarations() {
    assert_rejected(
        "fn u32_add(a: U32, b: U32): U32 { a }",
        "is a primitive operation",
    );
    assert_rejected(
        "enum U32 { only }
fn main(): U32 { 1u32 }",
        "is a primitive type",
    );
    assert_rejected(
        "enum Wrap { u32_mul(Nat) }
fn main(): Nat { 0 }",
        "is a primitive operation",
    );
}

#[test]
fn rejects_matching_on_u32() {
    assert_rejected(
        "fn f(n: U32): U32 { match n { x => x } }
fn main(): U32 { 0u32 }",
        "not an enum",
    );
}

#[test]
fn rejects_u32_literals_that_do_not_fit() {
    assert_rejected("fn main(): U32 { 4294967296u32 }", "too large");
}
