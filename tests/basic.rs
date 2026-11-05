use std::rc::Rc;

use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::{force, vapp};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::pretty::show_val;
use neo::value::V;

fn program(src: &str) -> (Globals, neo::surface::Program) {
    let globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    (globals, ast)
}

fn define(src: &str) -> Globals {
    let (mut globals, ast) = program(src);
    elaborate_program(&mut globals, &ast).expect("elaborate");
    globals
}

fn elaborate_error(src: &str) -> neo::error::Error {
    let (mut globals, ast) = program(src);
    elaborate_program(&mut globals, &ast).unwrap_err()
}

fn call(globals: &Globals, name: &str, args: &[V]) -> neo::value::V {
    let mut v: V = Rc::new(neo::value::Val::Glob(name.to_string()));
    for a in args {
        v = vapp(globals, v, a.clone());
    }
    force(globals, v)
}

fn nat(globals: &Globals, n: u64) -> V {
    let info = globals.get_enum("Nat").unwrap();
    let zero = info.ctors.iter().position(|c| c.name == "zero").unwrap();
    let succ = info.ctors.iter().position(|c| c.name == "succ").unwrap();
    let mut v = neo::value::vctor("Nat", zero, vec![]);
    for _ in 0..n {
        v = neo::value::vctor("Nat", succ, vec![v]);
    }
    v
}

fn as_nat(globals: &Globals, v: &V) -> u64 {
    let info = globals.get_enum("Nat").unwrap();
    let zero = info.ctors.iter().position(|c| c.name == "zero").unwrap();
    let succ = info.ctors.iter().position(|c| c.name == "succ").unwrap();
    fn go(v: &V, zero: usize, succ: usize) -> u64 {
        match &**v {
            neo::value::Val::Ctor(name, i, _args) if name == "Nat" && *i == zero => 0,
            neo::value::Val::Ctor(name, i, args) if name == "Nat" && *i == succ => {
                1 + go(&args[0], zero, succ)
            }
            other => panic!("not a nat: {:?}", other),
        }
    }
    go(v, zero, succ)
}

#[test]
fn prelude_installs() {
    let globals = new_globals().unwrap();
    assert!(globals.get_enum("Nat").is_some());
    assert!(globals.get_enum("Bool").is_some());
    assert!(globals.get_enum("List").is_some());
}

#[test]
fn add() {
    let globals = define(
        r#"
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}
"#,
    );
    let n = nat(&globals, 2);
    let m = nat(&globals, 3);
    let result = call(&globals, "add", &[n, m]);
    assert_eq!(as_nat(&globals, &result), 5);
    assert_eq!(show_val(&globals, 0, &result), "5");
}

#[test]
fn theorem_add_zero() {
    define(
        r#"
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}

fn sym(T: Type, x: T, y: T, p: x == y): y == x {
  match p {
    refl => refl
  }
}

fn add_zero(a: Nat): add(a, zero) == a {
  match a {
    zero => refl
    succ(p) => match sym(Nat, add(p, zero), p, add_zero(p)) {
      refl => refl
    }
  }
}
"#,
    );
}

#[test]
fn append_assoc() {
    define(
        r#"
fn append(T: Type, xs: List(T), ys: List(T)): List(T) {
  match xs {
    nil => ys
    cons(h, t) => cons(h, append(T, t, ys))
  }
}

fn append_assoc(T: Type, xs: List(T), ys: List(T), zs: List(T)): append(T, append(T, xs, ys), zs) == append(T, xs, append(T, ys, zs)) {
  match xs {
    nil => refl
    cons(h, t) => match append_assoc(T, t, ys, zs) {
      refl => refl
    }
  }
}
"#,
    );
}

#[test]
fn rejects_non_decreasing_recursion() {
    let (mut globals, ast) = program(
        r#"
fn loop(a: Nat): Nat {
  loop(a)
}
"#,
    );
    let err = elaborate_program(&mut globals, &ast).unwrap_err();
    assert!(err.message.contains("structurally decreasing") || err.message.contains("recursive"));
}

#[test]
fn rejects_prop_large_elimination() {
    let err = elaborate_error(
        r#"
fn bad(p: True): Nat {
  match p {
    trivial => 3
  }
}
"#,
    );
    assert!(
        err.message.contains("cannot eliminate this proposition"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn rejects_foreign_constructor_result() {
    let err = elaborate_error(
        r#"
enum E {
  c : Nat
}
"#,
    );
    assert!(
        err.message.contains("must return `E`"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn eliminates_empty_enum_with_zero_arms() {
    define(
        r#"
fn absurd(P: Prop, f: False): P {
  match f {
  }
}
"#,
    );
}

#[test]
fn rejects_zero_arm_match_on_inhabited_enum() {
    let err = elaborate_error(
        r#"
fn bad(n: Nat): Nat {
  match n {
  }
}
"#,
    );
    assert!(
        err.message.contains("not exhaustive"),
        "unexpected error: {}",
        err.message
    );
}
