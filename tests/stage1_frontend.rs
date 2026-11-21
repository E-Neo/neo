//! Stage 1 frontend and codegen slices.

#![cfg(not(target_family = "wasm"))]

use std::path::PathBuf;
use std::process::Command;

use neo::core::{PrimLit, const_};
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::{eval, force};
use neo::prelude::new_globals;
use neo::value::{V, Val};

fn with_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker");
}

fn load(relative: &str) -> Globals {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    let program = neo::load::parse_file(path).expect("load Stage 1 slice");
    let mut globals = new_globals().expect("prelude");
    elaborate_program(&mut globals, &program).expect("elaborate Stage 1 slice");
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
fn frontend_lexes_and_parses() {
    with_stack(|| {
        let globals = load("stage1/frontend/run.neo");
        assert_eq!(u32_of(&globals, "parser::demo_parse_add"), 6);
    });
}

#[test]
fn codegen_emits_valid_wasm() {
    with_stack(|| {
        let globals = load("stage1/codegen/run.neo");
        let value = call(&globals, "wasm::demo_module");
        let Val::PrimLit(PrimLit::Bytes(bytes)) = &*value else {
            panic!("wasm::demo_module is not Bytes: {value:?}");
        };
        assert_eq!(&bytes[..4], b"\0asm");
        if Command::new("wasm-tools")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let path =
            std::env::temp_dir().join(format!("neo-stage1-wasm-{}.wasm", std::process::id()));
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
    });
}

#[test]
fn driver_runs_as_a_component() {
    with_stack(|| {
        let globals = load("stage1/driver.neo");
        let dir = std::env::temp_dir().join(format!("neo-stage1-driver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::write(dir.join("input.neo"), "1 + 2 + 3").expect("input");
        let wasm = dir.join("driver.wasm");
        neo::codegen::build_component(&globals, &wasm).expect("build driver");
        let out = Command::new("wasmtime")
            .current_dir(&dir)
            .arg("--dir")
            .arg(".")
            .arg(&wasm)
            .arg("input.neo")
            .output()
            .expect("run driver");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "Ok(6)\n");
        assert_eq!(out.status.code(), Some(0));
    });
}
