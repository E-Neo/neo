//! Command-line exit-status tests.

#![cfg(not(target_family = "wasm"))]

use std::path::PathBuf;
use std::process::Command;

fn temp_source(name: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("neo-cli-{}-{name}.neo", std::process::id()));
    std::fs::write(&path, source).expect("write source");
    path
}

#[test]
fn interpreter_uses_a_u32_main_as_the_exit_status() {
    let path = temp_source(
        "status",
        r#"
fn main(): IO[U32] {
  IO::pure(7u32)
}
"#,
    );
    let out = Command::new(env!("CARGO_BIN_EXE_neo"))
        .arg(&path)
        .output()
        .expect("run neo");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "7\n");
    assert_eq!(out.status.code(), Some(7));
}

#[test]
fn interpreter_exits_zero_for_other_main_types() {
    let path = temp_source(
        "zero",
        r#"
fn main(): IO[Bool] {
  IO::pure(Bool::True)
}
"#,
    );
    let out = Command::new(env!("CARGO_BIN_EXE_neo"))
        .arg(&path)
        .output()
        .expect("run neo");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "True\n");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn child_file_errors_report_the_child_path() {
    let dir = std::env::temp_dir().join(format!("neo-cli-child-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    std::fs::write(dir.join("main.neo"), "mod child;\n").expect("main");
    std::fs::write(dir.join("child.neo"), "pub fn bad(): U32 {\n  True\n}\n").expect("child");
    let out = Command::new(env!("CARGO_BIN_EXE_neo"))
        .arg(dir.join("main.neo"))
        .output()
        .expect("run neo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("child.neo:"), "unexpected stderr: {stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn interpreter_handles_large_nat_literals() {
    let path = temp_source(
        "deep-nat",
        r#"
fn main(): IO[Nat] {
  IO::pure(2000)
}
"#,
    );
    let out = Command::new(env!("CARGO_BIN_EXE_neo"))
        .arg(&path)
        .output()
        .expect("run neo");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "2000\n");
    assert_eq!(out.status.code(), Some(0));
}
