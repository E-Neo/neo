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
