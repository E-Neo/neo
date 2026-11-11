#![cfg(not(target_family = "wasm"))]

use std::path::{Path, PathBuf};
use std::process::Command;

use neo::codegen::build_component;
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

fn main(): Nat {
  add(2, 3)
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

fn main(): Bool {
  not(false)
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

fn main(): Color {
  green
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
fn range(n: Nat): List(Nat) {
  match n {
    zero => nil
    succ(p) => cons(p, range(p))
  }
}

fn main(): List(Nat) {
  range(3)
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
fn main(): String {
  "hello, world"
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
fn main(): String {
  str_concat("hello, ", "world")
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

fn main(): Nat {
  addn(2)(3)
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
fn main(): Nat {
  f = succ
  f(4)
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
fn append(T: Type, xs: List(T), ys: List(T)): List(T) {
  match xs {
    nil => ys
    cons(h, t) => cons(h, append(T, t, ys))
  }
}

fn main(): List(Nat) {
  append(Nat, cons(1, cons(2, nil)), cons(3, nil))
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

fn main(): Nat {
  q = add_zero(3)
  7
}
"#,
        "proof",
        "7\n",
    );
}
