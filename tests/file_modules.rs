//! Tests for file-backed modules (`mod name;`) and cross-file re-exports.

#![cfg(not(target_family = "wasm"))]

use std::path::{Path, PathBuf};

use neo::core::{PrimLit, const_};
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::{eval, force};
use neo::host::{StdioHost, run_action};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::value::{V, Val};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("neo-files-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write(dir: &Path, relative: &str, source: &str) {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("module dir");
    }
    std::fs::write(path, source).expect("write module");
}

fn load(dir: &Path, root: &str) -> Globals {
    let program = neo::load::parse_file(dir.join(root)).expect("load");
    let mut globals = new_globals().expect("prelude");
    elaborate_program(&mut globals, &program).expect("elaborate");
    globals
}

fn rejection(dir: &Path, root: &str) -> neo::error::Error {
    let program = neo::load::parse_file(dir.join(root)).expect("load");
    let mut globals = new_globals().expect("prelude");
    elaborate_program(&mut globals, &program).unwrap_err()
}

fn run_main(globals: &Globals) -> V {
    let action = force(globals, eval(globals, &Vec::new(), &const_("main")));
    force(
        globals,
        run_action(globals, &mut StdioHost, &action).expect("run action"),
    )
}

fn main_u32(globals: &Globals) -> u32 {
    match &*run_main(globals) {
        Val::PrimLit(PrimLit::U32(n)) => *n,
        other => panic!("expected U32, got {other:?}"),
    }
}

#[test]
fn sibling_file_module() {
    let dir = temp_dir("sibling");
    write(
        &dir,
        "main.neo",
        r#"
mod util;

fn main(): IO[U32] {
  IO::pure(util::double(21u32))
}
"#,
    );
    write(
        &dir,
        "util.neo",
        r#"
pub fn double(x: U32): U32 {
  x * 2u32
}
"#,
    );
    assert_eq!(main_u32(&load(&dir, "main.neo")), 42);
}

#[test]
fn directory_module_and_its_children() {
    let dir = temp_dir("directory");
    write(
        &dir,
        "main.neo",
        r#"
mod util;

fn main(): IO[U32] {
  IO::pure(util::three())
}
"#,
    );
    write(
        &dir,
        "util/mod.neo",
        r#"
mod more;

pub fn three(): U32 {
  more::one() + 2u32
}
"#,
    );
    write(
        &dir,
        "util/more.neo",
        r#"
pub fn one(): U32 {
  1u32
}
"#,
    );
    assert_eq!(main_u32(&load(&dir, "main.neo")), 3);
}

#[test]
fn inline_module_children_live_under_its_name() {
    let dir = temp_dir("inline-child");
    write(
        &dir,
        "main.neo",
        r#"
mod outer {
  pub mod inner;
}

fn main(): IO[U32] {
  IO::pure(outer::inner::one())
}
"#,
    );
    write(
        &dir,
        "outer/inner.neo",
        r#"
pub fn one(): U32 {
  1u32
}
"#,
    );
    assert_eq!(main_u32(&load(&dir, "main.neo")), 1);
}

#[test]
fn re_exports_cross_file_boundaries() {
    let dir = temp_dir("re-export");
    write(
        &dir,
        "main.neo",
        r#"
pub mod shapes;
mod re;

use re::Circle

fn main(): IO[U32] {
  IO::pure(match Circle(7u32) {
    Circle(n) => n
  })
}
"#,
    );
    write(
        &dir,
        "shapes.neo",
        r#"
pub enum Shape {
  Circle(U32)
}
"#,
    );
    write(
        &dir,
        "re.neo",
        r#"
pub use super::shapes::Shape::{Circle}
"#,
    );
    assert_eq!(main_u32(&load(&dir, "main.neo")), 7);
}

#[test]
fn missing_module_file_is_reported() {
    let dir = temp_dir("missing");
    write(&dir, "main.neo", "mod absent;\n");
    let error = neo::load::parse_file(dir.join("main.neo")).unwrap_err();
    assert!(
        error.message.contains("cannot find module `absent`"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn ambiguous_module_layout_is_reported() {
    let dir = temp_dir("ambiguous");
    write(&dir, "main.neo", "mod both;\n");
    write(&dir, "both.neo", "pub fn f(): U32 { 0u32 }\n");
    write(&dir, "both/mod.neo", "pub fn f(): U32 { 0u32 }\n");
    let error = neo::load::parse_file(dir.join("main.neo")).unwrap_err();
    assert!(
        error.message.contains("is ambiguous"),
        "unexpected error: {}",
        error.message
    );
}

#[cfg(unix)]
#[test]
fn cyclic_module_files_are_reported() {
    use std::os::unix::fs::symlink;

    let dir = temp_dir("cycle");
    write(&dir, "main.neo", "mod a;\n");
    write(&dir, "a.neo", "mod a;\n");
    symlink(".", dir.join("a")).expect("symlink");
    let error = neo::load::parse_file(dir.join("main.neo")).unwrap_err();
    assert!(
        error.message.contains("cyclic module files"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn parse_source_rejects_unloaded_file_modules() {
    let program = parse_source("mod absent;\n").expect("parse");
    let mut globals = new_globals().expect("prelude");
    let error = elaborate_program(&mut globals, &program).unwrap_err();
    assert!(
        error.message.contains("was not loaded"),
        "unexpected error: {}",
        error.message
    );
}

#[test]
fn file_modules_share_visibility_rules() {
    let dir = temp_dir("visibility");
    write(
        &dir,
        "main.neo",
        r#"
mod secret;

fn main(): IO[U32] {
  IO::pure(secret::answer())
}
"#,
    );
    write(&dir, "secret.neo", "fn answer(): U32 { 42u32 }\n");
    let error = rejection(&dir, "main.neo");
    assert!(
        error.message.contains("is private here"),
        "unexpected error: {}",
        error.message
    );
}
