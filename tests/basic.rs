use std::rc::Rc;

use neo::core::PrimLit;
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::{force, vapp};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::pretty::show_val;
use neo::value::{V, Val};

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
    let zero = info.ctors.iter().position(|c| c.name == "Zero").unwrap();
    let succ = info.ctors.iter().position(|c| c.name == "Succ").unwrap();
    let mut v = neo::value::vctor("Nat", zero, vec![]);
    for _ in 0..n {
        v = neo::value::vctor("Nat", succ, vec![v]);
    }
    v
}

fn as_nat(globals: &Globals, v: &V) -> u64 {
    let info = globals.get_enum("Nat").unwrap();
    let zero = info.ctors.iter().position(|c| c.name == "Zero").unwrap();
    let succ = info.ctors.iter().position(|c| c.name == "Succ").unwrap();
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
    Zero => b
    Succ(p) => Succ(add(p, b))
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
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}

fn sym[T: Type](x: T, y: T, p: x = y): y = x {
  match p {
    Refl => Refl
  }
}

fn add_zero(a: Nat): add(a, Zero) = a {
  match a {
    Zero => Refl
    Succ(p) => match sym(add(p, Zero), p, add_zero(p)) {
      Refl => Refl
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
fn append[T: Type](xs: List[T], ys: List[T]): List[T] {
  match xs {
    Nil => ys
    Cons(h, t) => Cons(h, append(t, ys))
  }
}

fn append_assoc[T: Type](xs: List[T], ys: List[T], zs: List[T]): append(append(xs, ys), zs) = append(xs, append(ys, zs)) {
  match xs {
    Nil => Refl
    Cons(h, t) => match append_assoc(t, ys, zs) {
      Refl => Refl
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
  A
  B
}

fn bad(p: Two): Nat {
  match p {
    A => 0
    B => 1
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
  C : Nat
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
fn h(x: Nat, p: x = Zero): fn(y: Nat) -> Zero = x {
  match p {
    Refl => fn(y: Nat) => Refl
  }
}
"#,
    );
}

#[test]
fn equality_motive_compound_rhs_under_binder() {
    define(
        r#"
fn h(x: Nat, p: Succ(x) = Succ(Zero)): fn(y: Nat) -> Succ(Zero) = Succ(x) {
  match p {
    Refl => fn(y: Nat) => Refl
  }
}
"#,
    );
}

#[test]
fn transport_and_congruence() {
    define(
        r#"
fn cast[A: Type, B: Type](p: A = B, a: A): B {
  match p {
    Refl => a
  }
}

fn cong[T: Type, U: Type](f: fn(T) -> U, x: T, y: T, p: x = y): f(x) = f(y) {
  match p {
    Refl => Refl
  }
}
"#,
    );
}

#[test]
fn rejects_unsound_equality_motive() {
    let err = elaborate_error(
        r#"
fn bad(x: Nat, p: x = Zero): Zero = Succ(x) {
  match p {
    Refl => Refl
  }
}
"#,
    );
    assert!(
        err.message.contains("Refl"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn refines_indexed_branch_fields() {
    let globals = define(
        r#"
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn tail[T: Type, n: Nat](v: Vec[T, Succ(n)]): Vec[T, n] {
  match v {
    Vcons(m, x, xs) => xs
  }
}

fn demo(): Nat {
  match tail(Vcons(0, (5 : Nat), Vnil)) {
    Vnil => 9
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
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn head[T: Type, n: Nat](v: Vec[T, Succ(n)]): T {
  match v {
    Vcons(m, x, xs) => x
  }
}

fn demo(): Nat {
  head(Vcons(0, (7 : Nat), Vnil))
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

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    Vnil => Zero
    Vcons(m, x, xs) => Succ(length(xs))
  }
}

fn append[T: Type, n: Nat, m: Nat](xs: Vec[T, n], ys: Vec[T, m]): Vec[T, add(n, m)] {
  match xs {
    Vnil => ys
    Vcons(k, x, tail) => Vcons(add(k, m), x, append(tail, ys))
  }
}

fn two(): Vec[Nat, 2] {
  Vcons(1, 4, Vcons(0, 3, Vnil))
}

fn demo(): Nat {
  length(append(two(), Vcons(0, 7, Vnil)))
}
"#,
    );
    let result = call(&globals, "demo", &[]);
    assert_eq!(as_nat(&globals, &result), 3);
}

#[test]
fn infers_implicit_arguments() {
    let globals = define(
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

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    Vnil => Zero
    Vcons(m, x, xs) => Succ(length(xs))
  }
}

fn append[T: Type, n: Nat, m: Nat](xs: Vec[T, n], ys: Vec[T, m]): Vec[T, add(n, m)] {
  match xs {
    Vnil => ys
    Vcons(k, x, tail) => Vcons(add(k, m), x, append(tail, ys))
  }
}

fn v1(): Vec[Nat, 1] {
  Vcons(0, 10, Vnil)
}

fn v2(): Vec[Nat, 2] {
  Vcons(1, 20, Vcons(0, 30, Vnil))
}

fn both(): Vec[Nat, 3] {
  append(v1(), v2())
}

fn main(): Nat {
  add(length(both()), length(v2()))
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 5);
}

#[test]
fn infers_index_under_a_constructor() {
    let globals = define(
        r#"
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn head[T: Type, n: Nat](v: Vec[T, Succ(n)]): T {
  match v {
    Vcons(m, x, xs) => x
  }
}

fn v2(): Vec[Nat, 2] {
  Vcons(1, 20, Vcons(0, 30, Vnil))
}

fn main(): Nat {
  head(v2())
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 20);
}

#[test]
fn infers_type_parameters() {
    let globals = define(
        r#"
fn inc(n: Nat): Nat {
  Succ(n)
}

fn map[T: Type, U: Type](f: fn(T) -> U, xs: List[T]): List[U] {
  match xs {
    Nil => Nil
    Cons(x, rest) => Cons(f(x), map(f, rest))
  }
}

fn count(xs: List[Nat]): Nat {
  match xs {
    Nil => Zero
    Cons(x, rest) => Succ(count(rest))
  }
}

fn xs(): List[Nat] {
  Cons(1, Cons(2, Cons(3, Nil)))
}

fn main(): Nat {
  count(map(inc, xs()))
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 3);
}

#[test]
fn infers_leading_argument_from_expected_type() {
    let globals = define(
        r#"
fn empty[T: Type](): List[T] {
  Nil
}

fn count(xs: List[Nat]): Nat {
  match xs {
    Nil => Zero
    Cons(x, rest) => Succ(count(rest))
  }
}

fn empty_nat(): List[Nat] {
  empty()
}

fn main(): Nat {
  count(empty_nat())
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 0);
}

#[test]
fn supplies_implicit_arguments_explicitly() {
    let globals = define(
        r#"
fn id[T: Type](x: T): T {
  x
}

fn main(): Nat {
  id[Nat](4)
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 4);
}

#[test]
fn supplies_a_prefix_of_implicit_arguments() {
    let globals = define(
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

fn two(): Vec[Nat, 2] {
  Vcons(1, 20, Vcons(0, 30, Vnil))
}

fn main(): Nat {
  length[Nat](two())
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 2);
}

#[test]
fn implicit_binders_in_function_types_and_lambdas() {
    let globals = define(
        r#"
fn apply(f: fn[T: Type](x: T) -> T, x: Nat): Nat {
  f(x)
}

fn main(): Nat {
  apply(fn[T: Type](x: T) => x, 9)
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 9);
}

#[test]
fn implicit_binders_in_local_function_values() {
    let globals = define(
        r#"
fn id[T: Type](x: T): T {
  x
}

fn apply(f: fn(x: Nat) -> Nat, x: Nat): Nat {
  f(x)
}

fn main(): Nat {
  let local = fn[T: Type](x: T) => x
  let applied = apply(id, 3)
  local[Nat](4)
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 4);
}

#[test]
fn implicit_binders_survive_blocks_and_matches() {
    let globals = define(
        r#"
fn main(): Nat {
  let from_block = { fn[T: Type](x: T) => x }
  let from_match = match Bool::True {
    picked => fn[T: Type](x: T) => x
  }
  from_block(from_match[Nat](6))
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 6);
}

#[test]
fn implicit_lists_allow_newlines() {
    let globals = define(
        r#"
fn id[
  T: Type
](
  x: T
): T {
  x
}

fn main(): Nat {
  id[
    Nat
  ](5)
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert_eq!(as_nat(&globals, &result), 5);
}

#[test]
fn string_literals_and_operations() {
    let globals = define(
        r#"
fn main(): U32 {
  "hello".length() + "\n\t\\\"".length()
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert!(matches!(&*result, Val::PrimLit(PrimLit::U32(9))));
}

#[test]
fn string_equality_and_concat() {
    let globals = define(
        r#"
fn main(): Bool {
  "ab".concat("cd") == "abcd"
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert!(matches!(
        &*result,
        neo::value::Val::Ctor(name, index, _) if name == "Bool" && *index == 1
    ));
}

#[test]
fn string_slicing_and_concatenation() {
    let globals = define(
        r#"
fn main(): Bool {
  "hello, world".slice(7, 12).concat(" ".concat("héllo!".slice(0, 6))) == "world héllo"
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert!(matches!(
        &*result,
        neo::value::Val::Ctor(name, index, _) if name == "Bool" && *index == 1
    ));
}

#[test]
fn string_slices_are_total() {
    let globals = define(
        r#"
fn main(): U32 {
  "abc".slice(2, 1).length() +
  "abc".slice(99, 0).length() +
  "abc".slice(0, 99).length()
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert!(matches!(&*result, Val::PrimLit(PrimLit::U32(3))));
}

#[test]
fn bytes_operations() {
    let globals = define(
        r#"
fn main(): U32 {
  b"abc".get(1) +
  b"abcd".length() +
  b"a".get(9) +
  b"hello".slice(1, 3).concat(b"!").length()
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert!(matches!(&*result, Val::PrimLit(PrimLit::U32(105))));
}

#[test]
fn string_and_bytes_equality_operators() {
    let globals = define(
        r#"
fn strings(): Bool {
  "abc" == "ab".concat("c")
}

fn bytes(): Bool {
  b"a\x00" == "a\x00".to_bytes()
}
"#,
    );
    for name in ["strings", "bytes"] {
        let result = call(&globals, name, &[]);
        assert!(
            matches!(
                &*result,
                neo::value::Val::Ctor(name, index, _) if name == "Bool" && *index == 1
            ),
            "`{}` did not evaluate to true",
            name
        );
    }
}

#[test]
fn string_to_bytes_preserves_utf8() {
    let globals = define(
        r#"
fn main(): Bool {
  "héllo".to_bytes() == b"h\xc3\xa9llo"
}
"#,
    );
    let result = call(&globals, "main", &[]);
    assert!(matches!(
        &*result,
        neo::value::Val::Ctor(name, index, _) if name == "Bool" && *index == 1
    ));
}

#[test]
fn nested_patterns_select_ordered_arms() {
    let globals = define(
        r#"
fn classify(n: Nat): Nat {
  match n {
    Zero => Zero
    Succ(Zero) => 1
    Succ(Succ(Zero)) => 2
    Succ(Succ(Succ(k))) => 3
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
    Zero => b
    Succ(k) => Succ(add(k, b))
  }
}

fn inferred(n: Nat): Nat {
  let r: Nat = match n {
    Zero => 0
    Succ(Zero) => 1
    Succ(Succ(k)) => 2
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

fn two(v: Vec[Nat, 2]): Nat {
  match v {
    Vcons(Succ(Zero), x, Vcons(Zero, y, Vnil)) => add(x, y)
    Vcons(Succ(Zero), x, Vcons(Zero, y, Vcons(k, z, zs))) => z
  }
}

fn v2(): Vec[Nat, 2] {
  Vcons(1, 10, Vcons(0, 20, Vnil))
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
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn head[T: Type, n: Nat](v: Vec[T, Succ(n)]): T {
  match v {
    Vcons(m, x, xs) => x
  }
}

fn tail2[T: Type, n: Nat](v: Vec[T, Succ(Succ(n))]): Vec[T, n] {
  match v {
    Vcons(m, x, Vcons(k, y, ys)) => ys
  }
}

fn v3(): Vec[Nat, 3] {
  Vcons(2, 10, Vcons(1, 20, Vcons(0, 30, Vnil)))
}

fn main(): Nat {
  head(tail2(v3()))
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
  Leaf
  Node(Tree, Tree)
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(k) => Succ(add(k, b))
  }
}

fn count(t: Tree): Nat {
  match t {
    Leaf => Zero
    Node(Leaf, r) => Succ(count(r))
    Node(Node(a, b), r) => Succ(add(Succ(add(count(a), count(b))), count(r)))
  }
}

fn sample(): Tree {
  Node(Node(Leaf, Leaf), Node(Leaf, Node(Leaf, Leaf)))
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
enum Vec[A: Type, n: Nat] {
  Vnil : Vec[A, Zero]
  Vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, Succ(m)]
}

fn only_nil(v: Vec[Nat, Zero]): Nat {
  match v {
    Vnil => 1
  }
}

enum Fin[n: Nat] {
  FinZero(m: Nat) : Fin[Succ(m)]
  FinSucc(m: Nat, x: Fin[m]) : Fin[Succ(m)]
}

fn absurd(f: Fin[Zero]): Nat {
  match f {
  }
}
"#,
    );
}

#[test]
fn u32_to_byte_builds_bytes() {
    let globals = define(
        r#"
fn main(): Bytes {
  65u32.to_byte().concat(66u32.to_byte())
}
"#,
    );
    let result = call(&globals, "main", &[]);
    match &*result {
        Val::PrimLit(PrimLit::Bytes(bytes)) => assert_eq!(&**bytes, b"AB"),
        other => panic!("expected Bytes, got {other:?}"),
    }
}
