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
    if out.status.code().is_none() || !out.stderr.is_empty() {
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
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}

fn main(): IO[Nat] {
  IO::pure(add(2, 3))
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
    True => False
    False => True
  }
}

fn main(): IO[Bool] {
  IO::pure(not(False))
}
"#,
        "bool",
        "True\n",
    );
}

#[test]
fn prints_enum_values() {
    check(
        r#"
enum Color {
  Red
  Green
  Blue
}

fn main(): IO[Color] {
  IO::pure(Green)
}
"#,
        "color",
        "Green\n",
    );
}

#[test]
fn prints_recursive_enum_values() {
    check(
        r#"
fn range(n: Nat): List[Nat] {
  match n {
    Zero => Nil
    Succ(p) => Cons(p, range(p))
  }
}

fn main(): IO[List[Nat]] {
  IO::pure(range(3))
}
"#,
        "list",
        "Cons(2, Cons(1, Cons(0, Nil)))\n",
    );
}

#[test]
fn prints_strings() {
    check(
        r#"
fn main(): IO[String] {
  IO::pure("hello, world")
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
  IO::pure("hello, ".concat("world"))
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
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}

fn addn(n: Nat): fn(x: Nat) -> Nat {
  fn(x: Nat) => add(n, x)
}

fn main(): IO[Nat] {
  IO::pure(addn(2)(3))
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
  let f = Succ
  IO::pure(f(4))
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
    Nil => ys
    Cons(h, t) => Cons(h, append(t, ys))
  }
}

fn main(): IO[List[Nat]] {
  IO::pure(append(Cons((1 : Nat), Cons((2 : Nat), Nil)), Cons((3 : Nat), Nil)))
}
"#,
        "append",
        "Cons(1, Cons(2, Cons(3, Nil)))\n",
    );
}

#[test]
fn runs_proof_functions() {
    check(
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

fn main(): IO[Nat] {
  let q = add_zero(3)
  IO::pure(7)
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
  IO::pure(6u32 * 7u32 + 0u32 - 1u32)
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
  IO::pure(9u32 / 0u32 + 9u32 % 0u32)
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
  IO::pure(is_even(40u32))
}
"#,
        "u32-cmp",
        "True\n",
    );
}

#[test]
fn builds_nat_operators() {
    check(
        r#"
fn main(): IO[Nat] {
  IO::pure(((12 : Nat) & 10) + ((1 : Nat) << 4) + ((20 : Nat) / 6) + ((20 : Nat) % 6))
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
  IO::pure((20 / 6) + (20 % 6) + (1 << 4) + (12 & 10))
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
  IO::pure("hello, world".slice(7, 12).concat(" ".concat("héllo!".slice(0, 6))))
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
  IO::pure(
    b"abc".get(1) +
    b"abcd".length() +
    b"a".get(9) +
    b"hello".slice(1, 3).concat(b"!").length()
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
  IO::pure(b"neo".concat(b"\x00\xff"))
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
impl[T: Type] List[T] {
  pub fn length(xs: List[T]): Nat {
    match xs {
      Nil => Zero
      Cons(_, rest) => Succ(rest.length())
    }
  }
}

fn main(): IO[String] {
  IO::pure("hello".slice(0, 4).concat("!"))
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
impl[T: Type] List[T] {
  pub fn length(xs: List[T]): Nat {
    match xs {
      Nil => Zero
      Cons(_, rest) => Succ(rest.length())
    }
  }
}

fn main(): IO[Nat] {
  let xs: List[Nat] = Cons(1, Cons(2, Cons(3, Nil)))
  IO::pure(xs.length())
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
  IO::pure(b"hello, \xe2\x82\xac".to_str())
}
"#,
        "utf8-ok",
        "Ok(hello, €)\n",
    );
}

#[test]
fn builds_invalid_utf8_conversion() {
    check(
        r#"
fn main(): IO[Result[String, Utf8Error]] {
  IO::pure(b"ab\xff".to_str())
}
"#,
        "utf8-err",
        "Err(Invalid(2))\n",
    );
}

#[test]
fn builds_bytes_utf8_check() {
    check(
        r#"
fn code(r: Result[String, Utf8Error]): U32 {
  match r {
    Ok(_) => 0u32
    Err(Invalid(index)) => index + 1u32
  }
}

fn main(): IO[U32] {
  IO::pure(
    code(b"abc".to_str()) +
    code(b"\xf4\x8f\xbf\xbf".to_str()) * 10u32 +
    code(b"\xf4\x90\x80\x80".to_str())
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
  IO::bind(m, fn(x: U32) => IO::pure(x + 1u32))
}

fn main(): IO[U32] {
  inc(inc(IO::pure(40u32)))
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

/// Run a component with wasmtime and return both streams.
fn run_streams(path: &Path) -> Option<(String, String)> {
    let out = Command::new("wasmtime").arg(path).output().ok()?;
    if out.status.code().is_none() {
        panic!(
            "wasmtime failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

#[test]
fn builds_stderr_write() {
    let path = build(
        r#"
mod log {
  pub extern fn write(s: String): IO[Result[Unit, IoError]] = "wasi:cli/stderr@0.2.9#write"
}

fn exit_code(r: Result[Unit, IoError]): IO[U32] {
  match r {
    Ok(_) => IO::pure(0u32)
    Err(_) => IO::pure(1u32)
  }
}

fn main(): IO[U32] {
  IO::bind(log::write("hello\n"), exit_code)
}
"#,
        "stderr-write",
    );
    assert!(is_component(&path), "{} is not a component", path.display());
    if let Some((stdout, stderr)) = run_streams(&path) {
        assert_eq!(stdout, "0\n");
        assert_eq!(stderr, "hello\n");
    }
}

/// Run a component with a given working directory, arguments, and stdin.
fn run_capability(
    path: &Path,
    dir: Option<&Path>,
    args: &[&str],
    stdin: &[u8],
    envs: &[(&str, &str)],
) -> Option<String> {
    use std::io::Write as _;
    use std::process::Stdio;

    let mut command = Command::new("wasmtime");
    command
        .arg("--argv0")
        .arg("neo-program")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = dir {
        command.current_dir(dir).arg("--dir").arg(".");
    }
    for (name, value) in envs {
        command.arg("--env").arg(format!("{name}={value}"));
    }
    command.arg(path).args(args);
    let mut child = command.spawn().ok()?;
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(stdin)
        .expect("write stdin");
    let out = child.wait_with_output().ok()?;
    if out.status.code().is_none() || !out.stderr.is_empty() {
        panic!(
            "wasmtime failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn builds_argv() {
    let path = build(
        r#"
fn main(): IO[List[String]] {
  env::argv()
}
"#,
        "argv",
    );
    if let Some(stdout) = run_capability(&path, None, &["a", "b"], b"", &[]) {
        assert_eq!(stdout, "Cons(neo-program, Cons(a, Cons(b, Nil)))\n");
    }
}

#[test]
fn builds_stdin_read_all() {
    let path = build(
        r#"
fn main(): IO[Result[Bytes, IoError]] {
  stdin::read_all()
}
"#,
        "stdin-read-all",
    );
    if let Some(stdout) = run_capability(&path, None, &[], b"abc", &[]) {
        assert_eq!(stdout, "Ok(b\"\\x61\\x62\\x63\")\n");
    }
}

#[test]
fn builds_file_round_trip() {
    let dir = std::env::temp_dir().join(format!("neo-file-codegen-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = build(
        r#"
fn after(r: Result[Unit, IoError]): IO[Result[Bytes, IoError]] {
  match r {
    Ok(_) => file::read("round-trip.bin")
    Err(_) => IO::pure(Err(Closed))
  }
}

fn main(): IO[Result[Bytes, IoError]] {
  IO::bind(file::write("round-trip.bin", b"neo\x00file"), after)
}
"#,
        "file-round-trip",
    );
    if let Some(stdout) = run_capability(&path, Some(&dir), &[], b"", &[]) {
        assert_eq!(
            stdout,
            "Ok(b\"\\x6e\\x65\\x6f\\x00\\x66\\x69\\x6c\\x65\")\n"
        );
    }
    assert_eq!(
        std::fs::read(dir.join("round-trip.bin")).expect("written file"),
        b"neo\x00file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn run_exit(path: &Path) -> Option<(String, i32)> {
    let out = Command::new("wasmtime").arg(path).output().ok()?;
    assert!(
        out.stderr.is_empty(),
        "wasmtime failed for {}: {}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let code = out.status.code().expect("process exited normally");
    Some((String::from_utf8_lossy(&out.stdout).into_owned(), code))
}

#[test]
fn u32_main_sets_the_exit_status() {
    let path = build(
        r#"
fn main(): IO[U32] {
  IO::pure(7u32)
}
"#,
        "exit-status",
    );
    if let Some((stdout, code)) = run_exit(&path) {
        assert_eq!(stdout, "7\n");
        assert_eq!(code, 7);
    }
}

#[test]
fn non_u32_main_exits_zero() {
    let path = build(
        r#"
fn main(): IO[Bool] {
  IO::pure(Bool::True)
}
"#,
        "exit-status-zero",
    );
    if let Some((stdout, code)) = run_exit(&path) {
        assert_eq!(stdout, "True\n");
        assert_eq!(code, 0);
    }
}

#[test]
fn builds_stdout_write() {
    let path = build(
        r#"
fn main(): IO[Result[Unit, IoError]] {
  stdout::write("out\n")
}
"#,
        "stdout-write",
    );
    if let Some(stdout) = run_capability(&path, None, &[], b"", &[]) {
        assert_eq!(stdout, "out\nOk(Unit)\n");
    }
}

#[test]
fn builds_env_args() {
    let path = build(
        r#"
fn main(): IO[List[String]] {
  env::args()
}
"#,
        "env-args",
    );
    if let Some(stdout) = run_capability(&path, None, &["a", "b"], b"", &[]) {
        assert_eq!(stdout, "Cons(a, Cons(b, Nil))\n");
    }
}

#[test]
fn builds_env_get() {
    let path = build(
        r#"
fn after(r: Result[Unit, IoError]): IO[Option[String]] {
  match r {
    Ok(_) => env::get("NEO_TEST")
    Err(_) => IO::pure(None)
  }
}

fn main(): IO[Option[String]] {
  IO::bind(stdout::write("out\n"), after)
}
"#,
        "env-get",
    );
    if let Some(stdout) = run_capability(&path, None, &[], b"", &[("NEO_TEST", "value")]) {
        assert_eq!(stdout, "out\nSome(value)\n");
    }
}

#[test]
fn builds_dir_create_and_list() {
    let dir = std::env::temp_dir().join(format!("neo-dir-codegen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = build(
        r#"
fn after_create(r: Result[Unit, IoError]): IO[Result[List[String], IoError]] {
  match r {
    Ok(_) => dir::list(".")
    Err(e) => IO::pure(Err(e))
  }
}

fn main(): IO[Result[List[String], IoError]] {
  IO::bind(dir::create("newdir"), after_create)
}
"#,
        "dir-create-list",
    );
    if let Some(stdout) = run_capability(&path, Some(&dir), &[], b"", &[]) {
        assert_eq!(stdout, "Ok(Cons(newdir, Nil))\n");
    }
    assert!(dir.join("newdir").is_dir());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn builds_direct_and_tail_recursive_calls() {
    check(
        r#"
use nat_add

fn range(n: Nat, acc: List[Nat]): List[Nat] {
  match n {
    Zero => acc
    Succ(k) => range(k, Cons(k, acc))
  }
}

fn sum(xs: List[Nat], acc: Nat): Nat {
  match xs {
    Nil => acc
    Cons(x, rest) => sum(rest, nat_add(x, acc))
  }
}

fn main(): IO[Nat] {
  IO::pure(sum(range(100, Nil), Zero))
}
"#,
        "tail-recursion",
        "4950\n",
    );
}
