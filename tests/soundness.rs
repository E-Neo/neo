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
  let g = loop
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
  (fn(g: fn(Nat) -> Nat) => g(Zero))(boom)
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
    Zero => b
    Succ(p) => Succ(add(p, b))
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
    Zero => Zero
    Succ(p) => {
      let q = p
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
fn bad(): (fn(x: Type 0) -> Type 1) = (fn(x: Type 1) -> Type 1) {
  Refl
}
"#,
        "Refl",
    );
}

#[test]
fn rejects_type_field_in_small_enum() {
    assert_rejected(
        r#"
enum E[A: Type 1] : Type 0 {
  C(x: A)
}
"#,
        "larger than its enum",
    );
}

#[test]
fn rejects_negative_occurrence_in_constructor_index() {
    assert_rejected(
        r#"
enum Bad[F: Type 1] : Type 1 {
  Mk(g: F) : Bad[fn(x: Bad[F]) -> Type 0]
}
"#,
        "result index",
    );
}

#[test]
fn accepts_indexed_vec() {
    elaborate_ok(
        r#"
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}
"#,
    );
}

#[test]
fn accepts_indexed_length() {
    elaborate_ok(
        r#"
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    Vnil => Zero
    Vcons(m, x, xs) => Succ(length(xs))
  }
}
"#,
    );
}

#[test]
fn accepts_indexed_append() {
    elaborate_ok(
        r#"
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(k) => Succ(add(k, b))
  }
}

fn append[T: Type, n: Nat, m: Nat](xs: Vec[T, n], ys: Vec[T, m]): Vec[T, add(n, m)] {
  match xs {
    Vnil => ys
    Vcons(k, x, tail) => Vcons(add(k, m), x, append(tail, ys))
  }
}
"#,
    );
}

#[test]
fn accepts_fin_elimination() {
    elaborate_ok(
        r#"
enum Fin[n: Nat] {
  FinZero(m: Nat) : Fin[Succ(m)]
  FinSucc(m: Nat, f: Fin[m]) : Fin[Succ(m)]
}

fn fin_to_nat[n: Nat](f: Fin[n]): Nat {
  match f {
    FinZero(m) => Zero
    FinSucc(m, g) => Succ(fin_to_nat(g))
  }
}
"#,
    );
}

#[test]
fn accepts_impossible_branch_with_unchecked_body() {
    elaborate_ok(
        r#"
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn head[T: Type, n: Nat](v: Vec[T, Succ(n)]): T {
  match v {
    Vnil => False
    Vcons(m, x, xs) => x
  }
}
"#,
    );
}

#[test]
fn accepts_nested_constructor_patterns() {
    elaborate_ok(
        r#"
fn classify(n: Nat): Nat {
  match n {
    Zero => Zero
    Succ(Zero) => 1
    Succ(Succ(Zero)) => 2
    Succ(Succ(Succ(k))) => 3
  }
}

fn heads(xs: List[Nat]): Nat {
  match xs {
    Nil => Zero
    Cons(Zero, rest) => Zero
    Cons(Succ(k), Cons(x, rest)) => Succ(x)
    Cons(Succ(k), Nil) => k
  }
}
"#,
    );
}

#[test]
fn accepts_implicit_arguments() {
    elaborate_ok(
        r#"
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    Vnil => Zero
    Vcons(m, x, xs) => Succ(length(xs))
  }
}

fn v2(): Vec[Nat, 2] {
  Vcons(1, 20, Vcons(0, 30, Vnil))
}

fn main(): Nat {
  length(v2())
}
"#,
    );
}

#[test]
fn rejects_uninferable_implicit_argument() {
    assert_rejected(
        r#"
fn constant[T: Type](n: Nat): Nat {
  n
}

fn bad(): Nat {
  constant(3)
}
"#,
        "cannot infer implicit argument",
    );
}

#[test]
fn rejects_empty_implicit_parameter_list() {
    let err = parse_source(
        r#"
fn bad[](): Nat {
  0
}
"#,
    )
    .unwrap_err();
    assert!(
        err.message
            .contains("implicit parameter list cannot be empty"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn rejects_enum_parameters_in_parentheses() {
    assert_rejected(
        r#"
fn bad(): List(Nat) {
  Nil
}
"#,
        "written in `[]`",
    );
}

#[test]
fn rejects_implicit_argument_on_explicit_parameter() {
    assert_rejected(
        r#"
fn id(x: Nat): Nat {
  x
}

fn bad(): Nat {
  id[Nat](4)
}
"#,
        "no implicit parameter",
    );
}

#[test]
fn rejects_constructor_bracket_arguments() {
    assert_rejected(
        r#"
fn bad(): List[Nat] {
  Cons[Nat](1, Nil)
}
"#,
        "constructor type arguments are inferred",
    );
}

#[test]
fn rejects_too_many_implicit_arguments() {
    assert_rejected(
        r#"
fn id[T: Type](x: T): T {
  x
}

fn bad(): Nat {
  id[Nat, Nat](4)
}
"#,
        "too many implicit arguments",
    );
}

#[test]
fn rejects_non_exhaustive_nested_pattern() {
    assert_rejected(
        r#"
fn bad(n: Nat): Nat {
  match n {
    Zero => Zero
    Succ(Zero) => 1
  }
}
"#,
        "not exhaustive",
    );
}

#[test]
fn rejects_nested_pattern_with_wrong_arity() {
    assert_rejected(
        r#"
fn bad(n: Nat): Nat {
  match n {
    Zero => Zero
    Succ(Succ(Zero, Zero)) => 1
  }
}
"#,
        "field(s), but the pattern binds",
    );
}

#[test]
fn rejects_cyclic_index_refinement() {
    assert_rejected(
        r#"
enum T[a: Nat, b: Nat] {
  C(m: Nat) : T[m, Succ(m)]
}

fn bad[n: Nat](x: T[Succ(n), n]): Nat {
  match x {
    C(m) => Zero
  }
}
"#,
        "cannot refine constructor",
    );
}

#[test]
fn rejects_non_refinable_index_match() {
    assert_rejected(
        r#"
fn pred(n: Nat): Nat {
  match n {
    Zero => Zero
    Succ(k) => k
  }
}

enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn bad[n: Nat](v: Vec[Nat, pred(n)]): Nat {
  match v {
    Vnil => Zero
    Vcons(m, x, xs) => Zero
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
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn bad(v: Vec[Nat, Zero]): Nat {
  match v {
    Vcons(m, x, xs) => x
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
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn bad(): Vec[Nat, 1] {
  Vnil
}
"#,
        "produce the expected type",
    );
}

#[test]
fn rejects_misapplied_text_primitives() {
    assert_rejected(
        r#"fn main(): U32 { str_length("a", "b") }"#,
        "low-level primitive",
    );
    assert_rejected(r#"fn main(): U32 { "a".length(1) }"#, "takes 0 arguments");
    assert_rejected(
        r#"fn f(n: Nat): U32 { n.length() }"#,
        "no method `length` on type `Nat`",
    );
    assert_rejected(
        r#"fn main(): Bytes { bytes_get(b"a") }"#,
        "low-level primitive",
    );
    assert_rejected(
        r#"fn main(): String { str_slice("abc", 0u32) }"#,
        "low-level primitive",
    );
    assert_rejected(r#"fn main(): U32 { bytes_to_str(b"a") }"#, "unknown");
}

#[test]
fn rejects_arithmetic_and_matching_on_text() {
    assert_rejected(
        r#"fn main(): String { "a" + "b" }"#,
        "requires `Nat` or `U32`",
    );
    assert_rejected(
        r#"fn f(s: String): Nat { match s { x => Zero } }"#,
        "not an enum value",
    );
    assert_rejected(
        r#"fn f(s: Bytes): Nat { match s { x => Zero } }"#,
        "not an enum value",
    );
}

#[test]
fn rejects_equality_on_functions() {
    assert_rejected(
        r#"fn f(g: fn(Nat) -> Nat): Bool { g == g }"#,
        "boolean equality is not defined",
    );
}

#[test]
fn accepts_text_primitives() {
    elaborate_ok(
        r#"
fn main(): Bool {
  "abc".slice(1, 2).concat("!") == "b!"
}
"#,
    );
    elaborate_ok(
        r#"
fn main(): Bytes {
  b"abc".slice(0, 1).concat(b"\xff")
}
"#,
    );
}

#[test]
fn rejects_text_primitive_redeclarations() {
    assert_rejected(
        "enum String { Only }\nfn main(): Nat { 0 }",
        "is a primitive type",
    );
    assert_rejected(
        "fn bytes_slice(b: Bytes): Bytes { b }",
        "is a primitive operation",
    );
    assert_rejected(
        "enum Wrap { str_length(Nat) }\nfn main(): Nat { 0 }",
        "is a primitive operation",
    );
}
