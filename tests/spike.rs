//! Readiness spike: fuel-driven evaluation, byte parsing, and a pure emitter.

#![cfg(not(target_family = "wasm"))]

use std::path::PathBuf;
use std::process::Command;

use neo::core::{PrimLit, const_};
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::{eval, force};
use neo::host::{StdioHost, run_action};
use neo::prelude::new_globals;
use neo::value::{V, Val};

fn globals() -> Globals {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spike/run.neo");
    let program = neo::load::parse_file(path).expect("load spike");
    let mut globals = new_globals().expect("prelude");
    elaborate_program(&mut globals, &program).expect("elaborate spike");
    globals
}

fn call(globals: &Globals, name: &str) -> V {
    force(globals, eval(globals, &Vec::new(), &const_(name)))
}

fn u32_of(globals: &Globals, name: &str) -> u32 {
    match &*call(globals, name) {
        Val::PrimLit(PrimLit::U32(n)) => *n,
        other => panic!("`{name}` is not U32: {other:?}"),
    }
}

#[test]
fn spike_slices_run() {
    let globals = globals();
    assert_eq!(u32_of(&globals, "core::demo"), 42);
    assert_eq!(u32_of(&globals, "parser::demo_parse"), 123);
    assert_eq!(u32_of(&globals, "emit::demo_len"), 12);
    assert_eq!(u32_of(&globals, "closures::demo2"), 7);
}

#[test]
fn spike_main_runs() {
    let globals = globals();
    let action = force(&globals, eval(&globals, &Vec::new(), &const_("main")));
    let value = force(
        &globals,
        run_action(&globals, &mut StdioHost, &action).expect("run main"),
    );
    assert!(matches!(&*value, Val::PrimLit(PrimLit::U32(184))));
}

#[test]
fn emitted_module_is_valid_wasm() {
    let globals = globals();
    let value = call(&globals, "emit::demo_module");
    let Val::PrimLit(PrimLit::Bytes(bytes)) = &*value else {
        panic!("emit::demo_module is not Bytes: {value:?}");
    };
    assert_eq!(&bytes[..4], b"\0asm");
    if Command::new("wasm-tools")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let path = std::env::temp_dir().join(format!("neo-spike-{}.wasm", std::process::id()));
    std::fs::write(&path, &**bytes).expect("write module");
    let out = Command::new("wasm-tools")
        .arg("validate")
        .arg(&path)
        .output()
        .expect("run wasm-tools");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "wasm-tools rejected the module: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn spike_component_runs() {
    let globals = globals();
    let path = std::env::temp_dir().join(format!("neo-spike-run-{}.wasm", std::process::id()));
    neo::codegen::build_component(&globals, &path).expect("build spike component");
    let out = Command::new("wasmtime")
        .arg(&path)
        .output()
        .expect("run wasmtime");
    let _ = std::fs::remove_file(&path);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "184\n");
    assert_eq!(out.status.code(), Some(184));
}
