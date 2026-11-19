#![cfg(not(target_family = "wasm"))]

use std::rc::Rc;

use neo::core::PrimLit;
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::force;
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::value::{V, Val};

fn define(src: &str) -> Globals {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).expect("elaborate");
    globals
}

fn elaborate_error(src: &str) -> neo::error::Error {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).unwrap_err()
}

fn main_value(globals: &Globals) -> V {
    force(globals, Rc::new(Val::Glob("main".to_string())))
}

fn as_nat(globals: &Globals, v: &V) -> u64 {
    let info = globals.get_enum("Nat").unwrap();
    let zero = info.ctors.iter().position(|c| c.name == "zero").unwrap();
    let succ = info.ctors.iter().position(|c| c.name == "succ").unwrap();
    fn go(v: &V, zero: usize, succ: usize) -> u64 {
        match &**v {
            Val::Ctor(name, i, _) if name == "Nat" && *i == zero => 0,
            Val::Ctor(name, i, args) if name == "Nat" && *i == succ => 1 + go(&args[0], zero, succ),
            other => panic!("not a Nat: {other:?}"),
        }
    }
    go(v, zero, succ)
}

fn assert_rejected(src: &str, needle: &str) {
    let err = elaborate_error(src);
    assert!(
        err.message.contains(needle),
        "expected error containing {needle:?}, got {:?}",
        err.message
    );
}

#[test]
fn string_methods_chain() {
    let globals = define(
        r#"
fn main(): String {
  "hello, world".slice(7, 12).concat("!")
}
"#,
    );
    match &*main_value(&globals) {
        Val::PrimLit(PrimLit::Str(bytes)) => assert_eq!(&**bytes, b"world!"),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn bytes_methods() {
    let globals = define(
        r#"
fn main(): U32 {
  b"abc".get(1) + b"abcd".length() + b"xy".concat(b"!").length()
}
"#,
    );
    match &*main_value(&globals) {
        Val::PrimLit(PrimLit::U32(n)) => assert_eq!(*n, 105),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn generic_list_length() {
    let globals = define(
        r#"
fn List.length[T: Type](xs: List[T]): Nat {
  match xs {
    nil => zero
    cons(_, rest) => succ(rest.length())
  }
}

fn main(): Nat {
  let xs: List[Nat] = cons(1, cons(2, cons(3, nil)))
  xs.length()
}
"#,
    );
    assert_eq!(as_nat(&globals, &main_value(&globals)), 3);
}

#[test]
fn explicit_implicit_method_argument() {
    let globals = define(
        r#"
fn List.length[T: Type](xs: List[T]): Nat {
  match xs {
    nil => zero
    cons(_, rest) => succ(rest.length())
  }
}

fn main(): Nat {
  let xs: List[Nat] = cons(1, nil)
  xs.length[Nat]()
}
"#,
    );
    assert_eq!(as_nat(&globals, &main_value(&globals)), 1);
}

#[test]
fn user_defined_methods_on_enums() {
    let globals = define(
        r#"
fn Bool.not(self: Bool): Bool {
  match self {
    false => true
    true => false
  }
}

enum Shape {
  circle(U32)
  rect(U32, U32)
}

fn Shape.area(self: Shape): U32 {
  match self {
    circle(r) => r * r * 3u32
    rect(w, h) => w * h
  }
}

fn main(): U32 {
  match false.not() {
    true => rect(3u32, 4u32).area()
    false => 0u32
  }
}
"#,
    );
    match &*main_value(&globals) {
        Val::PrimLit(PrimLit::U32(n)) => assert_eq!(*n, 12),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn overloaded_names_are_selected_by_receiver_type() {
    let globals = define(
        r#"
fn List.length[T: Type](xs: List[T]): Nat {
  match xs {
    nil => zero
    cons(_, rest) => succ(rest.length())
  }
}

fn main(): U32 {
  "abcd".length() + b"xy".length()
}
"#,
    );
    match &*main_value(&globals) {
        Val::PrimLit(PrimLit::U32(n)) => assert_eq!(*n, 6),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn methods_on_primitive_types() {
    let globals = define(
        r#"
fn U32.inc(self: U32): U32 {
  self + 1u32
}

fn main(): U32 {
  41u32.inc()
}
"#,
    );
    match &*main_value(&globals) {
        Val::PrimLit(PrimLit::U32(n)) => assert_eq!(*n, 42),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn rejects_unknown_method() {
    assert_rejected(
        "fn main(): Nat { zero.length() }",
        "no method `length` on type `Nat`",
    );
}

#[test]
fn rejects_method_on_wrong_receiver_type() {
    assert_rejected(
        r#"fn main(): U32 { "a".get(0) }"#,
        "no method `get` on type `String`",
    );
}

#[test]
fn rejects_method_on_type_variable() {
    assert_rejected(
        r#"
fn f[T: Type](x: T): U32 { x.length() }
fn main(): U32 { 0 }
"#,
        "cannot select a method on type",
    );
}

#[test]
fn rejects_method_without_receiver() {
    assert_rejected(
        "fn String.length(): U32 { 0 }\nfn main(): U32 { 0 }",
        "needs a receiver parameter",
    );
}

#[test]
fn rejects_method_with_wrong_receiver_parameter() {
    assert_rejected(
        "fn String.length(self: Bytes): U32 { 0 }\nfn main(): U32 { 0 }",
        "must have type `String`",
    );
}

#[test]
fn rejects_method_on_non_type_namespace() {
    assert_rejected(
        "fn Nope.length(self: Nope): U32 { 0 }\nfn main(): U32 { 0 }",
        "is not a type",
    );
}

#[test]
fn rejects_duplicate_method() {
    assert_rejected(
        r#"
fn String.length(self: String): U32 { 0u32 }
fn String.length(self: String): U32 { 1u32 }
fn main(): U32 { 0 }
"#,
        "`String.length` is already defined",
    );
}

#[test]
fn rejects_wrong_method_arity() {
    assert_rejected(
        r#"fn main(): U32 { "a".length(1) }"#,
        "takes 0 arguments after the receiver, got 1",
    );
}

#[test]
fn rejects_non_decreasing_method_recursion() {
    assert_rejected(
        r#"
fn List.loop[T: Type](xs: List[T]): Nat {
  xs.loop()
}
fn main(): Nat { zero }
"#,
        "not structurally decreasing",
    );
}

#[test]
fn method_can_return_a_function() {
    let globals = define(
        r#"
fn Nat.apply_twice(self: Nat): fn(x: Nat) -> Nat {
  fn(x: Nat) => nat_add(self, nat_add(x, self))
}

fn main(): Nat {
  (2 : Nat).apply_twice()(3)
}
"#,
    );
    assert_eq!(as_nat(&globals, &main_value(&globals)), 7);
}

#[test]
fn generic_receiver_on_a_user_enum() {
    let globals = define(
        r#"
enum Box[T: Type] {
  box(T)
}

fn Box.get[T: Type](self: Box[T]): T {
  match self {
    box(n) => n
  }
}

fn main(): Nat {
  let b: Box[Nat] = box(3)
  b.get()
}
"#,
    );
    assert_eq!(as_nat(&globals, &main_value(&globals)), 3);
}

#[test]
fn method_dispatch_needs_an_inferable_receiver() {
    assert_rejected(
        r#"
enum Box[T: Type] {
  box(T)
}

fn Box.get[T: Type](self: Box[T]): T {
  match self {
    box(n) => n
  }
}

fn main(): Nat {
  box(3).get()
}
"#,
        "cannot infer the type arguments of `box`",
    );
}
