//! Tests for the `IO` modality: actions are values, and only the entry point
//! interprets them.

#![cfg(not(target_family = "wasm"))]

use neo::core::const_;
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::{eval, force};
use neo::host::{Host, run_action};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::pretty::show_val;
use neo::value::{V, Val};

fn define(src: &str) -> Globals {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).expect("elaborate");
    globals
}

fn elaborate_error(src: &str) -> neo::error::Error {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).unwrap_err()
}

fn run(globals: &Globals, name: &str) -> V {
    let action = force(globals, eval(globals, &Vec::new(), &const_(name)));
    let mut host = NoHost;
    force(
        globals,
        run_action(globals, &mut host, &action).expect("run action"),
    )
}

/// The `IO` tests do not use host capabilities.
struct NoHost;

impl Host for NoHost {
    fn run(&mut self, _capability: &neo::host::Capability, _args: &[V]) -> neo::error::Result<V> {
        Err(neo::error::Error::new("unexpected host capability"))
    }
}

#[test]
fn pure_action_produces_its_value() {
    let globals = define("fn answer(): IO[U32] { IO::pure(42u32) }");
    assert_eq!(show_val(&globals, 0, &run(&globals, "answer")), "42");
}

#[test]
fn bind_threads_the_produced_value() {
    let globals = define(
        r#"
fn inc(m: IO[U32]): IO[U32] {
  IO::bind(m, fn(x: U32) => IO::pure(x + 1u32))
}

fn answer(): IO[U32] {
  inc(IO::pure(41u32))
}
"#,
    );
    assert_eq!(show_val(&globals, 0, &run(&globals, "answer")), "42");
}

#[test]
fn bind_is_polymorphic() {
    let globals = define(
        r#"
fn second[T: Type](first: IO[T], next: IO[T]): IO[T] {
  IO::bind(first, fn(x: T) => IO::bind(next, fn(y: T) => IO::pure(y)))
}

fn value(): IO[String] {
  second(IO::pure("ignored"), IO::pure("kept"))
}
"#,
    );
    assert_eq!(show_val(&globals, 0, &run(&globals, "value")), "\"kept\"");
}

#[test]
fn actions_are_not_enum_values() {
    let error = elaborate_error(
        r#"
fn bad(m: IO[U32]): U32 {
  match m {
  }
}
"#,
    );
    assert!(
        error.message.contains("match"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn bind_requires_an_action() {
    let error = elaborate_error(
        r#"
fn bad(): IO[U32] {
  IO::bind(1u32, fn(x: U32) => IO::pure(x))
}
"#,
    );
    assert!(
        error.message.contains("not an `IO` action"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn bind_checks_the_continuation_domain() {
    let error = elaborate_error(
        r#"
fn bad(): IO[U32] {
  IO::bind(IO::pure(1u32), fn(x: Nat) => IO::pure(0u32))
}
"#,
    );
    assert!(
        error.message.contains("does not match"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn io_expects_a_type_parameter() {
    let error = elaborate_error("fn bad(): IO { IO::pure(0u32) }");
    assert!(
        error.message.contains("`IO` expects one parameter"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn actions_are_opaque_values() {
    let globals = define("fn action(): IO[U32] { IO::pure(1u32) }");
    let value = force(&globals, eval(&globals, &Vec::new(), &const_("action")));
    assert!(matches!(&*value, Val::Action(_)));
}
