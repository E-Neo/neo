use neo::elab::elaborate_program;
use neo::parser::parse_source;
use neo::prelude::new_globals;

fn elaborate_err(src: &str) -> neo::error::Error {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).unwrap_err()
}

fn elaborate_ok(src: &str) {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).expect("elaborate");
}

fn assert_rejected(src: &str, needle: &str) {
    let err = elaborate_err(src);
    assert!(
        err.message.contains(needle),
        "expected error containing {:?}, got {:?}",
        needle,
        err.message
    );
}

#[test]
fn rejects_escaped_recursive_function() {
    assert_rejected(
        r#"
fn apply(g: fn(Nat) -> False, x: Nat): False { g(x) }
fn boom(n: Nat): False { apply(boom, n) }
"#,
        "used as a value",
    );
}

#[test]
fn rejects_direct_non_decreasing_recursion() {
    assert_rejected(
        r#"
fn loop(n: Nat): Nat { loop(n) }
"#,
        "structurally decreasing",
    );
}

#[test]
fn rejects_parenthesised_recursive_head() {
    assert_rejected(
        r#"
fn loop(n: Nat): Nat { (loop)(n) }
"#,
        "structurally decreasing",
    );
}

#[test]
fn rejects_recursive_alias() {
    assert_rejected(
        r#"
fn loop(n: Nat): Nat {
  g = loop
  g(n)
}
"#,
        "structurally decreasing",
    );
}

#[test]
fn rejects_lambda_escaped_recursive_call() {
    assert_rejected(
        r#"
fn boom(n: Nat): Nat {
  (fn(g: fn(Nat) -> Nat) => g(zero))(boom)
}
"#,
        "structurally decreasing",
    );
}

#[test]
fn accepts_structural_recursion() {
    elaborate_ok(
        r#"
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}
"#,
    );
}

#[test]
fn accepts_smaller_field_through_alias() {
    elaborate_ok(
        r#"
fn f(n: Nat): Nat {
  match n {
    zero => zero
    succ(p) => {
      q = p
      f(q)
    }
  }
}
"#,
    );
}

#[test]
fn rejects_universe_cumulativity_in_pi_domain() {
    assert_rejected(
        r#"
fn id0(x: Type 0): Type 0 { x }
fn apply1(h: fn(Type 1) -> Type 0): Type 0 { h(Type 0) }
fn bad(): Type 0 { apply1(id0) }
"#,
        "type mismatch",
    );
}

#[test]
fn rejects_equal_pi_types_with_different_domains() {
    assert_rejected(
        r#"
fn bad(): (fn(x: Type 0) -> Type 1) == (fn(x: Type 1) -> Type 1) {
  refl
}
"#,
        "refl",
    );
}

#[test]
fn rejects_type_field_in_small_enum() {
    assert_rejected(
        r#"
enum E(A: Type 1) : Type 0 {
  c(x: A)
}
"#,
        "larger than its enum",
    );
}

#[test]
fn rejects_negative_occurrence_in_constructor_index() {
    assert_rejected(
        r#"
enum Bad(F: Type 1) : Type 1 {
  mk(g: F) : Bad(fn(x: Bad(F)) -> Type 0)
}
"#,
        "result index",
    );
}

#[test]
fn accepts_indexed_vec() {
    elaborate_ok(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}
"#,
    );
}

#[test]
fn rejects_bare_parameter_index_match() {
    assert_rejected(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn length(n: Nat, v: Vec(Nat, n)): Nat {
  match v {
    vnil => 0
    vcons(m, x, xs) => 1
  }
}
"#,
        "cannot refine constructor",
    );
}

#[test]
fn rejects_missing_reachable_indexed_constructor() {
    assert_rejected(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn bad(v: Vec(Nat, zero)): Nat {
  match v {
    vcons(m, x, xs) => x
  }
}
"#,
        "not exhaustive",
    );
}

#[test]
fn rejects_indexed_constructor_with_wrong_index() {
    assert_rejected(
        r#"
enum Vec(A: Type, n: Nat) {
  vnil : Vec(A, zero)
  vcons(m: Nat, x: A, xs: Vec(A, m)) : Vec(A, succ(m))
}

fn bad(): Vec(Nat, 1) {
  vnil
}
"#,
        "produce the expected type",
    );
}
