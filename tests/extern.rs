//! Tests for `extern` host capabilities.

#![cfg(not(target_family = "wasm"))]

use neo::core::{PrimLit, const_};
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::{eval, force};
use neo::host::{Capability, Host, Lowering, StderrOutcome, run_action, stderr_result};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::pretty::show_val;
use neo::value::{V, Val};

const STDERR_WRITE: &str = r#"mod log {
  pub extern fn write(s: String): IO[Result[Unit, IoError]] = "wasi:cli/stderr@0.2.9#write"
}"#;

fn define(source: &str) -> Globals {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(source).expect("parse");
    elaborate_program(&mut globals, &ast).expect("elaborate");
    globals
}

fn elaborate_error(source: &str) -> neo::error::Error {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(source).expect("parse");
    elaborate_program(&mut globals, &ast).unwrap_err()
}

fn assert_error(source: &str, needle: &str) {
    let error = elaborate_error(source);
    assert!(
        error.message.contains(needle),
        "expected an error containing `{}`, got `{}`",
        needle,
        error.message
    );
}

struct RecordingHost {
    writes: Vec<Vec<u8>>,
    outcome: StderrOutcome,
}

impl Host for RecordingHost {
    fn run(&mut self, capability: &Capability, args: &[V]) -> neo::error::Result<V> {
        assert_eq!(capability.lowering, Lowering::StderrWrite);
        let [arg] = args else {
            panic!("`log::write` takes one argument");
        };
        match &**arg {
            Val::PrimLit(PrimLit::Str(bytes)) => self.writes.push(bytes.to_vec()),
            other => panic!("unexpected argument: {other:?}"),
        }
        Ok(stderr_result(self.outcome))
    }
}

fn run_main(globals: &Globals, outcome: StderrOutcome) -> (V, Vec<Vec<u8>>) {
    let action = force(globals, eval(globals, &Vec::new(), &const_("main")));
    let mut host = RecordingHost {
        writes: Vec::new(),
        outcome,
    };
    let value = force(
        globals,
        run_action(globals, &mut host, &action).expect("run action"),
    );
    (value, host.writes)
}

const PROGRAM: &str = r#"
fn exit_code(r: Result[Unit, IoError]): IO[U32] {
  match r {
    Ok(_) => IO::pure(0u32)
    Err(_) => IO::pure(1u32)
  }
}

fn main(): IO[U32] {
  IO::bind(log::write("hello\n"), exit_code)
}
"#;

#[test]
fn a_later_declaration_cannot_take_a_namespace() {
    assert_error(
        &format!("{STDERR_WRITE}\nfn log(): U32 {{ 0u32 }}"),
        "already defined",
    );
    assert_error(
        &format!("{STDERR_WRITE}\nenum log {{ X }}"),
        "already defined",
    );
}

#[test]
fn scoped_extern_call_runs_once() {
    let globals = define(&format!("{STDERR_WRITE}\n{PROGRAM}"));
    let (value, writes) = run_main(&globals, StderrOutcome::Ok);
    assert_eq!(show_val(&globals, 0, &value), "0");
    assert_eq!(writes, vec![b"hello\n".to_vec()]);
}

#[test]
fn closed_stream_is_visible_to_the_program() {
    let globals = define(&format!("{STDERR_WRITE}\n{PROGRAM}"));
    let (value, writes) = run_main(&globals, StderrOutcome::Closed);
    assert_eq!(show_val(&globals, 0, &value), "1");
    assert_eq!(writes.len(), 1);
}

#[test]
fn unknown_import_is_rejected() {
    assert_error(
        r#"mod log {
  pub extern fn write(s: String): IO[Result[Unit, IoError]] = "wasi:cli/stderr@0.2.9#nope"
}"#,
        "unknown host capability",
    );
}

#[test]
fn declared_type_must_match_the_capability() {
    assert_error(
        r#"mod log {
  pub extern fn write(s: String): IO[Unit] = "wasi:cli/stderr@0.2.9#write"
}"#,
        "must have type",
    );
}

#[test]
fn extern_must_return_io() {
    assert_error(
        r#"mod log {
  pub extern fn write(s: String): Unit = "wasi:cli/stderr@0.2.9#write"
}"#,
        "must return an `IO` action",
    );
}

#[test]
fn implicit_parameters_are_rejected() {
    assert_error(
        r#"mod log {
  pub extern fn write[T: Type](s: String): IO[Result[Unit, IoError]] = "wasi:cli/stderr@0.2.9#write"
}"#,
        "may not be implicit",
    );
}

#[test]
fn only_primitive_parameters_are_allowed() {
    assert_error(
        r#"mod log {
  pub extern fn write(s: Option[String]): IO[Result[Unit, IoError]] = "wasi:cli/stderr@0.2.9#write"
}"#,
        "must have a primitive type",
    );
}

#[test]
fn a_type_cannot_be_a_capability_namespace() {
    assert_error(
        r#"mod Bool {
  pub extern fn write(s: String): IO[Result[Unit, IoError]] = "wasi:cli/stderr@0.2.9#write"
}"#,
        "is a type",
    );
}

#[test]
fn duplicate_externs_are_rejected() {
    assert_error(
        &format!("{STDERR_WRITE}\n{STDERR_WRITE}"),
        "is already defined",
    );
}

#[test]
fn a_capability_namespace_is_not_a_value() {
    let error = elaborate_error(&format!("{STDERR_WRITE}\nfn bad(): U32 {{ log }}"));
    assert!(
        error.message.contains("is a module"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn extern_arity_is_checked() {
    let globals_src = format!(
        "{STDERR_WRITE}\nfn bad(): IO[Result[Unit, IoError]] {{ log::write(\"a\", \"b\") }}"
    );
    let error = elaborate_error(&globals_src);
    assert!(
        error.message.contains("too many") || error.message.contains("too few"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn extern_arguments_are_checked() {
    let src = format!("{STDERR_WRITE}\nfn bad(): IO[Result[Unit, IoError]] {{ log::write(1u32) }}");
    let error = elaborate_error(&src);
    assert!(
        error.message.contains("type mismatch") || error.message.contains("expected"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn extern_actions_are_structural_values() {
    let globals = define(&format!(
        r#"{STDERR_WRITE}
fn first(): IO[Result[Unit, IoError]] {{ log::write("a") }}
fn second(): IO[Result[Unit, IoError]] {{ log::write("a") }}
"#
    ));
    let a = eval(&globals, &Vec::new(), &const_("first"));
    let b = eval(&globals, &Vec::new(), &const_("second"));
    let a = force(&globals, a);
    let b = force(&globals, b);
    assert_eq!(show_val(&globals, 0, &a), show_val(&globals, 0, &b));
}
