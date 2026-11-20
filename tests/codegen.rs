#![cfg(not(target_family = "wasm"))]

use std::path::{Path, PathBuf};
use std::process::Command;

use neo::codegen::{build_component, build_component_bytes};
use neo::elab::elaborate_program;
use neo::parser::parse_source;
use neo::prelude::new_globals;

fn build(source: &str, name: &str) -> PathBuf {
    let mut globals = new_globals().expect("prelude");
    let program = parse_source(source).expect("parse");
    elaborate_program(&mut globals, &program).expect("elaborate");

    let dir = std::env::temp_dir().join("neo-codegen-tests");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join(format!("{}-{}.wasm", name, std::process::id()));
    build_component(&globals, &path).expect("codegen");
    path
}

fn build_error(source: &str) -> neo::error::Error {
    let mut globals = new_globals().expect("prelude");
    let program = parse_source(source).expect("parse");
    elaborate_program(&mut globals, &program).expect("elaborate");
    build_component_bytes(&globals).expect_err("codegen should fail")
}

fn is_component(path: &Path) -> bool {
    let bytes = std::fs::read(path).expect("read component");
    bytes.len() >= 8 && &bytes[0..4] == b"\0asm" && bytes[4..8] == [0x0d, 0x00, 0x01, 0x00]
}

/// Run a component with wasmtime. Returns `None` when wasmtime is unavailable
/// so the test can be skipped in minimal environments.
fn run(path: &Path) -> Option<String> {
    let out = Command::new("wasmtime").arg(path).output().ok()?;
    if !out.status.success() {
        panic!(
            "wasmtime failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn check(source: &str, name: &str, expected: &str) {
    let path = build(source, name);
    assert!(is_component(&path), "{} is not a component", path.display());
    if let Some(stdout) = run(&path) {
        assert_eq!(stdout, expected, "unexpected output from {}", name);
    }
}

#[test]
fn builds_nat_main() {
    check(
        r#"
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}

fn main(): IO[Nat] {
  io_pure(add(2, 3))
}
"#,
        "nat",
        "5\n",
    );
}

#[test]
fn builds_bool_main() {
    check(
        r#"
fn not(b: Bool): Bool {
  match b {
    true => false
    false => true
  }
}

fn main(): IO[Bool] {
  io_pure(not(false))
}
"#,
        "bool",
        "true\n",
    );
}

#[test]
fn prints_enum_values() {
    check(
        r#"
enum Color {
  red
  green
  blue
}

fn main(): IO[Color] {
  io_pure(green)
}
"#,
        "color",
        "green\n",
    );
}

#[test]
fn prints_recursive_enum_values() {
    check(
        r#"
fn range(n: Nat): List[Nat] {
  match n {
    zero => nil
    succ(p) => cons(p, range(p))
  }
}

fn main(): IO[List[Nat]] {
  io_pure(range(3))
}
"#,
        "list",
        "cons(2, cons(1, cons(0, nil)))\n",
    );
}

#[test]
fn prints_strings() {
    check(
        r#"
fn main(): IO[String] {
  io_pure("hello, world")
}
"#,
        "string",
        "hello, world\n",
    );
}

#[test]
fn prints_concatenated_strings() {
    check(
        r#"
fn main(): IO[String] {
  io_pure(str_concat("hello, ", "world"))
}
"#,
        "string-concat",
        "hello, world\n",
    );
}

#[test]
fn supports_closures_and_captures() {
    check(
        r#"
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}

fn addn(n: Nat): fn(x: Nat) -> Nat {
  fn(x: Nat) => add(n, x)
}

fn main(): IO[Nat] {
  io_pure(addn(2)(3))
}
"#,
        "closure",
        "5\n",
    );
}

#[test]
fn supports_first_class_constructors() {
    check(
        r#"
fn main(): IO[Nat] {
  let f = succ
  io_pure(f(4))
}
"#,
        "ctor",
        "5\n",
    );
}

#[test]
fn compiles_generic_functions() {
    check(
        r#"
fn append[T: Type](xs: List[T], ys: List[T]): List[T] {
  match xs {
    nil => ys
    cons(h, t) => cons(h, append(t, ys))
  }
}

fn main(): IO[List[Nat]] {
  io_pure(append(cons((1 : Nat), cons((2 : Nat), nil)), cons((3 : Nat), nil)))
}
"#,
        "append",
        "cons(1, cons(2, cons(3, nil)))\n",
    );
}

#[test]
fn runs_proof_functions() {
    check(
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

fn main(): IO[Nat] {
  let q = add_zero(3)
  io_pure(7)
}
"#,
        "proof",
        "7\n",
    );
}

#[test]
fn builds_u32_arithmetic() {
    check(
        r#"
fn main(): IO[U32] {
  io_pure(6u32 * 7u32 + 0u32 - 1u32)
}
"#,
        "u32",
        "41\n",
    );
}

#[test]
fn builds_u32_division_by_zero() {
    check(
        r#"
fn main(): IO[U32] {
  io_pure(9u32 / 0u32 + 9u32 % 0u32)
}
"#,
        "u32-divzero",
        "0\n",
    );
}

#[test]
fn builds_u32_comparison() {
    check(
        r#"
fn is_even(n: U32): Bool {
  n % 2u32 == 0u32
}

fn main(): IO[Bool] {
  io_pure(is_even(40u32))
}
"#,
        "u32-cmp",
        "true\n",
    );
}

#[test]
fn builds_nat_operators() {
    check(
        r#"
fn main(): IO[Nat] {
  io_pure(((12 : Nat) & 10) + ((1 : Nat) << 4) + ((20 : Nat) / 6) + ((20 : Nat) % 6))
}
"#,
        "nat-operators",
        "29\n",
    );
}

#[test]
fn builds_u32_infix_operators() {
    check(
        r#"
fn main(): IO[U32] {
  io_pure((20 / 6) + (20 % 6) + (1 << 4) + (12 & 10))
}
"#,
        "u32-infix-operators",
        "29\n",
    );
}

#[test]
fn builds_string_slicing_and_concatenation() {
    check(
        r#"
fn main(): IO[String] {
  io_pure(str_concat(str_slice("hello, world", 7, 12), str_concat(" ", str_slice("héllo!", 0, 6))))
}
"#,
        "string-slice",
        "world héllo\n",
    );
}

#[test]
fn builds_bytes_operations() {
    check(
        r#"
fn main(): IO[U32] {
  io_pure(
    bytes_get(b"abc", 1) +
    bytes_length(b"abcd") +
    bytes_get(b"a", 9) +
    bytes_length(bytes_concat(bytes_slice(b"hello", 1, 3), b"!"))
  )
}
"#,
        "bytes-ops",
        "105\n",
    );
}

#[test]
fn builds_bytes_printing() {
    check(
        r#"
fn main(): IO[Bytes] {
  io_pure(bytes_concat(b"neo", b"\x00\xff"))
}
"#,
        "bytes-print",
        "b\"\\x6e\\x65\\x6f\\x00\\xff\"\n",
    );
}

#[test]
fn builds_method_calls() {
    check(
        r#"
fn List.length[T: Type](xs: List[T]): Nat {
  match xs {
    nil => zero
    cons(_, rest) => succ(rest.length())
  }
}

fn main(): IO[String] {
  io_pure("hello".slice(0, 4).concat("!"))
}
"#,
        "methods",
        "hell!\n",
    );
}

#[test]
fn builds_generic_method_calls() {
    check(
        r#"
fn List.length[T: Type](xs: List[T]): Nat {
  match xs {
    nil => zero
    cons(_, rest) => succ(rest.length())
  }
}

fn main(): IO[Nat] {
  let xs: List[Nat] = cons(1, cons(2, cons(3, nil)))
  io_pure(xs.length())
}
"#,
        "generic-methods",
        "3\n",
    );
}

#[test]
fn builds_valid_utf8_conversion() {
    check(
        r#"
fn main(): IO[Result[String, Utf8Error]] {
  io_pure(b"hello, \xe2\x82\xac".to_str())
}
"#,
        "utf8-ok",
        "ok(hello, €)\n",
    );
}

#[test]
fn builds_invalid_utf8_conversion() {
    check(
        r#"
fn main(): IO[Result[String, Utf8Error]] {
  io_pure(b"ab\xff".to_str())
}
"#,
        "utf8-err",
        "err(invalid_utf8(2))\n",
    );
}

#[test]
fn builds_bytes_utf8_check() {
    check(
        r#"
fn main(): IO[U32] {
  io_pure(
    bytes_utf8_check(b"abc") +
    bytes_utf8_check(b"\xf4\x8f\xbf\xbf") * 10u32 +
    bytes_utf8_check(b"\xf4\x90\x80\x80")
  )
}
"#,
        "utf8-check",
        "1\n",
    );
}

#[test]
fn builds_io_bind() {
    check(
        r#"
fn inc(m: IO[U32]): IO[U32] {
  io_bind(m, fn(x: U32) => io_pure(x + 1u32))
}

fn main(): IO[U32] {
  inc(inc(io_pure(40u32)))
}
"#,
        "io-bind",
        "42\n",
    );
}

#[test]
fn rejects_non_io_main() {
    let error = build_error(
        r#"
fn main(): U32 {
  0u32
}
"#,
    );
    assert!(
        error.message.contains("`main` must have type `IO[T]`"),
        "unexpected error: {}",
        error.message
    );
}
