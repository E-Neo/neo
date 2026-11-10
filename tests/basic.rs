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
enum Two : Prop {
  a
  b
}

fn bad(p: Two): Nat {
  match p {
    a => 0
    b => 1
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

#[test]
fn equality_motive_under_binder() {
    define(
        r#"
fn h(x: Nat, p: x == zero): fn(y: Nat) -> zero == x {
  match p {
    refl => fn(y: Nat) => refl
  }
}
"#,
    );
}

#[test]
fn equality_motive_compound_rhs_under_binder() {
    define(
        r#"
fn h(x: Nat, p: succ(x) == succ(zero)): fn(y: Nat) -> succ(zero) == succ(x) {
  match p {
    refl => fn(y: Nat) => refl
  }
}
"#,
    );
}

#[test]
fn transport_and_congruence() {
    define(
        r#"
fn cast(A: Type, B: Type, p: A == B, a: A): B {
  match p {
    refl => a
  }
}

fn cong(T: Type, U: Type, f: fn(T) -> U, x: T, y: T, p: x == y): f(x) == f(y) {
  match p {
    refl => refl
  }
}
"#,
    );
}

#[test]
fn rejects_unsound_equality_motive() {
    let err = elaborate_error(
        r#"
fn bad(x: Nat, p: x == zero): zero == succ(x) {
  match p {
    refl => refl
  }
}
"#,
    );
    assert!(
        err.message.contains("refl"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn refines_indexed_branch_fields() {
    let globals = define(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn tail(n: Nat, v: Vec(Nat, succ(n))): Vec(Nat, n) {
  match v {
    vcons(m, x, xs) => xs
  }
}

fn demo(): Nat {
  match tail(0, vcons(0, 5, vnil)) {
    vnil => 9
  }
}
"#,
    );
    let result = call(&globals, "demo", &[]);
    assert_eq!(as_nat(&globals, &result), 9);
}

#[test]
fn refines_field_from_context_index() {
    let globals = define(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn head(n: Nat, v: Vec(Nat, succ(n))): Nat {
  match v {
    vcons(m, x, xs) => x
  }
}

fn demo(): Nat {
  head(0, vcons(0, 7, vnil))
}
"#,
    );
    let result = call(&globals, "demo", &[]);
    assert_eq!(as_nat(&globals, &result), 7);
}

#[test]
fn refines_context_index_in_append() {
    let globals = define(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn length(n: Nat, v: Vec(Nat, n)): Nat {
  match v {
    vnil => zero
    vcons(m, x, xs) => succ(length(m, xs))
  }
}

fn append(n: Nat, m: Nat, xs: Vec(Nat, n), ys: Vec(Nat, m)): Vec(Nat, add(n, m)) {
  match xs {
    vnil => ys
    vcons(k, x, tail) => vcons(add(k, m), x, append(k, m, tail, ys))
  }
}

fn two(): Vec(Nat, 2) {
  vcons(1, 4, vcons(0, 3, vnil))
}

fn demo(): Nat {
  length(3, append(2, 1, two(), vcons(0, 7, vnil)))
}
"#,
    );
    let result = call(&globals, "demo", &[]);
    assert_eq!(as_nat(&globals, &result), 3);
}

#[test]
fn nested_patterns_select_ordered_arms() {
    let globals = define(
        r#"
fn classify(n: Nat): Nat {
  match n {
    zero => zero
    succ(zero) => 1
    succ(succ(zero)) => 2
    succ(succ(succ(k))) => 3
  }
}

fn main(): Nat {
  classify(3)
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 3);
}

#[test]
fn nested_patterns_infer_result_type() {
    let globals = define(
        r#"
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn inferred(n: Nat): Nat {
  r = match n {
    zero => 0
    succ(zero) => 1
    succ(succ(k)) => 2
  }
  r
}

fn main(): Nat {
  add(inferred(2), inferred(1))
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 3);
}

#[test]
fn nested_patterns_decompose_indexed_values() {
    let globals = define(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn two(v: Vec(Nat, 2)): Nat {
  match v {
    vcons(succ(zero), x, vcons(zero, y, vnil)) => add(x, y)
    vcons(succ(zero), x, vcons(zero, y, vcons(k, z, zs))) => z
  }
}

fn v2(): Vec(Nat, 2) {
  vcons(1, 10, vcons(0, 20, vnil))
}

fn main(): Nat {
  two(v2())
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 30);
}

#[test]
fn nested_patterns_infer_dependent_index() {
    let globals = define(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn head(n: Nat, v: Vec(Nat, succ(n))): Nat {
  match v {
    vcons(m, x, xs) => x
  }
}

fn tail2(n: Nat, v: Vec(Nat, succ(succ(n)))): Vec(Nat, n) {
  match v {
    vcons(m, x, vcons(k, y, ys)) => ys
  }
}

fn v3(): Vec(Nat, 3) {
  vcons(2, 10, vcons(1, 20, vcons(0, 30, vnil)))
}

fn main(): Nat {
  head(0, tail2(1, v3()))
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 30);
}

#[test]
fn nested_patterns_recursing_on_inner_fields() {
    let globals = define(
        r#"
enum Tree {
  leaf
  node(Tree, Tree)
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn count(t: Tree): Nat {
  match t {
    leaf => zero
    node(leaf, r) => succ(count(r))
    node(node(a, b), r) => succ(add(succ(add(count(a), count(b))), count(r)))
  }
}

fn sample(): Tree {
  node(node(leaf, leaf), node(leaf, node(leaf, leaf)))
}

fn main(): Nat {
  count(sample())
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 4);
}

#[test]
fn omits_impossible_indexed_branch() {
    define(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn only_nil(v: Vec(Nat, zero)): Nat {
  match v {
    vnil => 1
  }
}

enum Fin(n: Nat) {
  fzero(m: Nat) : Fin(succ(m))
  fsucc(m: Nat, x: Fin(m)) : Fin(succ(m))
}

fn absurd(f: Fin(zero)): Nat {
  match f {
  }
}
"#,
    );
}
