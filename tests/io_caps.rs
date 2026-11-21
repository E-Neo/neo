//! Tests for `env`, `stdin`, and `file` host capabilities.

#![cfg(not(target_family = "wasm"))]

use neo::core::{PrimLit, const_};
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::{eval, force};
use neo::host::{StdioHost, run_action};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::value::{V, Val};

fn define(source: &str) -> Globals {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(source).expect("parse");
    elaborate_program(&mut globals, &ast).expect("elaborate");
    globals
}

fn run_main(globals: &Globals) -> V {
    let action = force(globals, eval(globals, &Vec::new(), &const_("main")));
    force(
        globals,
        run_action(globals, &mut StdioHost, &action).expect("run action"),
    )
}

fn ctor_fields<'a>(globals: &Globals, enum_name: &str, index: usize, args: &'a [V]) -> &'a [V] {
    let ctor = &globals.get_enum(enum_name).expect("enum").ctors[index];
    &args[ctor.num_params..]
}

fn file_program(path: &str) -> String {
    format!(
        r#"
fn after(r: Result[Unit, IoError]): IO[Result[Bytes, IoError]] {{
  match r {{
    Ok(_) => file::read("{path}")
    Err(_) => IO::pure(Err(Closed))
  }}
}}

fn main(): IO[Result[Bytes, IoError]] {{
  IO::bind(file::write("{path}", b"neo\x00file"), after)
}}
"#
    )
}

#[test]
fn file_write_then_read() {
    let dir = std::env::temp_dir().join(format!("neo-file-caps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("round-trip.bin");
    let path = file.to_string_lossy();
    let globals = define(&file_program(&path));
    let value = run_main(&globals);
    let Val::Ctor(name, index, args) = &*value else {
        panic!("expected a Result, got {value:?}");
    };
    assert_eq!(name, "Result");
    assert_eq!(*index, 0);
    let fields = ctor_fields(&globals, "Result", 0, args);
    match &*fields[0] {
        Val::PrimLit(PrimLit::Bytes(bytes)) => assert_eq!(&**bytes, b"neo\x00file"),
        other => panic!("expected Bytes, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn file_read_missing_is_an_error() {
    let path = std::env::temp_dir().join(format!(
        "neo-file-missing-{}-does-not-exist.bin",
        std::process::id()
    ));
    let globals = define(&format!(
        r#"
fn main(): IO[Result[Bytes, IoError]] {{
  file::read("{}")
}}
"#,
        path.to_string_lossy()
    ));
    let value = run_main(&globals);
    let Val::Ctor(name, index, _) = &*value else {
        panic!("expected a Result, got {value:?}");
    };
    assert_eq!(name, "Result");
    assert_eq!(*index, 1);
}

#[test]
fn argv_is_a_list_of_strings() {
    let globals = define(
        r#"
fn main(): IO[List[String]] {
  env::argv()
}
"#,
    );
    let value = run_main(&globals);
    let Val::Ctor(name, index, args) = &*value else {
        panic!("expected a List, got {value:?}");
    };
    assert_eq!(name, "List");
    assert_eq!(*index, 1, "the process argument vector is non-empty");
    let fields = ctor_fields(&globals, "List", 1, args);
    assert!(matches!(&*fields[0], Val::PrimLit(PrimLit::Str(_))));
}
