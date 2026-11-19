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

fn sym[T: Type](x: T, y: T, p: x = y): y = x {
  match p {
    refl => refl
  }
}

fn add_zero(a: Nat): add(a, zero) = a {
  match a {
    zero => refl
    succ(p) => match sym(add(p, zero), p, add_zero(p)) {
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
fn append[T: Type](xs: List[T], ys: List[T]): List[T] {
  match xs {
    nil => ys
    cons(h, t) => cons(h, append(t, ys))
  }
}

fn append_assoc[T: Type](xs: List[T], ys: List[T], zs: List[T]): append(append(xs, ys), zs) = append(xs, append(ys, zs)) {
  match xs {
    nil => refl
    cons(h, t) => match append_assoc(t, ys, zs) {
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
fn h(x: Nat, p: x = zero): fn(y: Nat) -> zero = x {
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
fn h(x: Nat, p: succ(x) = succ(zero)): fn(y: Nat) -> succ(zero) = succ(x) {
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
fn cast[A: Type, B: Type](p: A = B, a: A): B {
  match p {
    refl => a
  }
}

fn cong[T: Type, U: Type](f: fn(T) -> U, x: T, y: T, p: x = y): f(x) = f(y) {
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
fn bad(x: Nat, p: x = zero): zero = succ(x) {
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
enum Vec[A: Type, n: Nat] {
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn tail[T: Type, n: Nat](v: Vec[T, succ(n)]): Vec[T, n] {
  match v {
    vcons(m, x, xs) => xs
  }
}

fn demo(): Nat {
  match tail(vcons(0, (5 : Nat), vnil)) {
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
enum Vec[A: Type, n: Nat] {
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn head[T: Type, n: Nat](v: Vec[T, succ(n)]): T {
  match v {
    vcons(m, x, xs) => x
  }
}

fn demo(): Nat {
  head(vcons(0, (7 : Nat), vnil))
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
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    vnil => zero
    vcons(m, x, xs) => succ(length(xs))
  }
}

fn append[T: Type, n: Nat, m: Nat](xs: Vec[T, n], ys: Vec[T, m]): Vec[T, add(n, m)] {
  match xs {
    vnil => ys
    vcons(k, x, tail) => vcons(add(k, m), x, append(tail, ys))
  }
}

fn two(): Vec[Nat, 2] {
  vcons(1, 4, vcons(0, 3, vnil))
}

fn demo(): Nat {
  length(append(two(), vcons(0, 7, vnil)))
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
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    vnil => zero
    vcons(m, x, xs) => succ(length(xs))
  }
}

fn append[T: Type, n: Nat, m: Nat](xs: Vec[T, n], ys: Vec[T, m]): Vec[T, add(n, m)] {
  match xs {
    vnil => ys
    vcons(k, x, tail) => vcons(add(k, m), x, append(tail, ys))
  }
}

fn v1(): Vec[Nat, 1] {
  vcons(0, 10, vnil)
}

fn v2(): Vec[Nat, 2] {
  vcons(1, 20, vcons(0, 30, vnil))
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
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn head[T: Type, n: Nat](v: Vec[T, succ(n)]): T {
  match v {
    vcons(m, x, xs) => x
  }
}

fn v2(): Vec[Nat, 2] {
  vcons(1, 20, vcons(0, 30, vnil))
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
  succ(n)
}

fn map[T: Type, U: Type](f: fn(T) -> U, xs: List[T]): List[U] {
  match xs {
    nil => nil
    cons(x, rest) => cons(f(x), map(f, rest))
  }
}

fn count(xs: List[Nat]): Nat {
  match xs {
    nil => zero
    cons(x, rest) => succ(count(rest))
  }
}

fn xs(): List[Nat] {
  cons(1, cons(2, cons(3, nil)))
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
  nil
}

fn count(xs: List[Nat]): Nat {
  match xs {
    nil => zero
    cons(x, rest) => succ(count(rest))
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
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    vnil => zero
    vcons(m, x, xs) => succ(length(xs))
  }
}

fn two(): Vec[Nat, 2] {
  vcons(1, 20, vcons(0, 30, vnil))
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
  let from_match = match true {
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
  str_length("hello") + str_length("\n\t\\\"")
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
  str_eq(str_concat("ab", "cd"), "abcd")
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
  str_eq(
    str_concat(str_slice("hello, world", 7, 12), str_concat(" ", str_slice("héllo!", 0, 6))),
    "world héllo"
  )
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
  str_length(str_slice("abc", 2, 1)) +
  str_length(str_slice("abc", 99, 0)) +
  str_length(str_slice("abc", 0, 99))
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
  bytes_get(b"abc", 1) +
  bytes_length(b"abcd") +
  bytes_get(b"a", 9) +
  bytes_length(bytes_concat(bytes_slice(b"hello", 1, 3), b"!"))
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
  "abc" == str_concat("ab", "c")
}

fn bytes(): Bool {
  b"a\x00" == str_to_bytes("a\x00")
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
  bytes_eq(str_to_bytes("héllo"), b"h\xc3\xa9llo")
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
  let r: Nat = match n {
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
enum Vec[A: Type, n: Nat] {
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn two(v: Vec[Nat, 2]): Nat {
  match v {
    vcons(succ(zero), x, vcons(zero, y, vnil)) => add(x, y)
    vcons(succ(zero), x, vcons(zero, y, vcons(k, z, zs))) => z
  }
}

fn v2(): Vec[Nat, 2] {
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
enum Vec[A: Type, n: Nat] {
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn head[T: Type, n: Nat](v: Vec[T, succ(n)]): T {
  match v {
    vcons(m, x, xs) => x
  }
}

fn tail2[T: Type, n: Nat](v: Vec[T, succ(succ(n))]): Vec[T, n] {
  match v {
    vcons(m, x, vcons(k, y, ys)) => ys
  }
}

fn v3(): Vec[Nat, 3] {
  vcons(2, 10, vcons(1, 20, vcons(0, 30, vnil)))
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
enum Vec[A: Type, n: Nat] {
  vnil : Vec[A, zero]
  vcons(m: Nat, x: A, xs: Vec[A, m]) : Vec[A, succ(m)]
}

fn only_nil(v: Vec[Nat, zero]): Nat {
  match v {
    vnil => 1
  }
}

enum Fin[n: Nat] {
  fzero(m: Nat) : Fin[succ(m)]
  fsucc(m: Nat, x: Fin[m]) : Fin[succ(m)]
}

fn absurd(f: Fin[zero]): Nat {
  match f {
  }
}
"#,
    );
}
