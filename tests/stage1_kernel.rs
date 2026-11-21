//! Stage 1 kernel slice: evaluation and quoting in Neo.

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

fn with_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker");
}

fn globals() -> Globals {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stage1/kernel/run.neo");
    let program = neo::load::parse_file(path).expect("load Stage 1 kernel");
    let mut globals = new_globals().expect("prelude");
    elaborate_program(&mut globals, &program).expect("elaborate Stage 1 kernel");
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
fn kernel_slices_pass() {
    with_stack(|| {
        let globals = globals();
        assert_eq!(u32_of(&globals, "core::demo_identity"), 1);
        assert_eq!(u32_of(&globals, "core::demo_pi"), 1);
        assert_eq!(u32_of(&globals, "core::demo_list"), 1);
        assert_eq!(u32_of(&globals, "core::demo_unbound"), 1);
        assert_eq!(u32_of(&globals, "core::demo_fuel"), 1);
        assert_eq!(u32_of(&globals, "core::demo_conv_alpha"), 1);
        assert_eq!(u32_of(&globals, "core::demo_conv_diff"), 1);
        assert_eq!(u32_of(&globals, "core::demo_check_identity"), 1);
        assert_eq!(u32_of(&globals, "core::demo_check_bad"), 1);
        assert_eq!(u32_of(&globals, "core::demo_infer_pi"), 1);
        assert_eq!(u32_of(&globals, "core::demo_infer_app"), 1);
        assert_eq!(u32_of(&globals, "core::demo_infer_ind_bool"), 1);
        assert_eq!(u32_of(&globals, "core::demo_infer_ctor_bool"), 1);
        assert_eq!(u32_of(&globals, "core::demo_check_ctor_bool"), 1);
        assert_eq!(u32_of(&globals, "core::demo_match_eval"), 1);
        assert_eq!(u32_of(&globals, "core::demo_match_check"), 1);
        assert_eq!(u32_of(&globals, "core::demo_match_missing"), 1);
        assert_eq!(u32_of(&globals, "core::demo_match_arity"), 1);
        assert_eq!(u32_of(&globals, "core::demo_eq_check"), 1);
        assert_eq!(u32_of(&globals, "core::demo_eq_bad"), 1);
        assert_eq!(u32_of(&globals, "core::demo_j_eval"), 1);
        assert_eq!(u32_of(&globals, "core::demo_j_check"), 1);
        assert_eq!(u32_of(&globals, "core::demo_positive_bool"), 1);
        assert_eq!(u32_of(&globals, "core::demo_negative_positivity"), 1);
        assert_eq!(u32_of(&globals, "core::demo_nested_positivity"), 1);
        assert_eq!(u32_of(&globals, "core::demo_show_type"), 1);
        assert_eq!(u32_of(&globals, "core::demo_show_lambda"), 1);
        assert_eq!(u32_of(&globals, "core::demo_show_pi"), 1);
        assert_eq!(u32_of(&globals, "core::demo_show_app"), 1);
        assert_eq!(u32_of(&globals, "core::demo_termination_good"), 1);
        assert_eq!(u32_of(&globals, "core::demo_termination_loop"), 1);
        assert_eq!(u32_of(&globals, "core::demo_termination_succ"), 1);
        assert_eq!(u32_of(&globals, "core::demo_infer_match"), 1);
        assert_eq!(u32_of(&globals, "core::demo_infer_j"), 1);
    });
}

#[test]
fn kernel_main_runs() {
    with_stack(|| {
        let globals = globals();
        let action = force(&globals, eval(&globals, &Vec::new(), &const_("main")));
        let value = force(
            &globals,
            run_action(&globals, &mut StdioHost, &action).expect("run kernel main"),
        );
        assert!(matches!(&*value, Val::PrimLit(PrimLit::U32(34))));
    });
}

#[test]
fn kernel_component_runs() {
    with_stack(|| {
        let globals = globals();
        let path =
            std::env::temp_dir().join(format!("neo-stage1-kernel-{}.wasm", std::process::id()));
        neo::codegen::build_component(&globals, &path).expect("build kernel component");
        let out = Command::new("wasmtime")
            .arg(&path)
            .output()
            .expect("run wasmtime");
        let _ = std::fs::remove_file(&path);
        assert_eq!(String::from_utf8_lossy(&out.stdout), "34\n");
        assert_eq!(out.status.code(), Some(34));
    });
}
