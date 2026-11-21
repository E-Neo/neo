//! Tests for modules, visibility, paths, and `impl` blocks.

#![cfg(not(target_family = "wasm"))]

use neo::core::PrimLit;
use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::error::Error;
use neo::eval::{eval, force};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::value::{V, Val};

fn elaborate(src: &str) -> Globals {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).expect("elaborate");
    globals
}

fn rejection(src: &str) -> Error {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).unwrap_err()
}

fn assert_rejected(src: &str, needle: &str) {
    let err = rejection(src);
    assert!(
        err.message.contains(needle),
        "expected error containing {:?}, got {:?}",
        needle,
        err.message
    );
}

fn main_value(globals: &Globals) -> V {
    let mut current = force(
        globals,
        eval(globals, &Vec::new(), &neo::core::const_("main")),
    );
    loop {
        let name = match &*current {
            Val::Glob(name) => name.clone(),
            _ => return current,
        };
        current = force(globals, current);
        if matches!(&*current, Val::Glob(n) if *n == name) {
            return current;
        }
    }
}

fn main_u32(src: &str) -> u32 {
    let globals = elaborate(src);
    match &*main_value(&globals) {
        Val::PrimLit(PrimLit::U32(n)) => *n,
        other => panic!("`main` did not evaluate to a u32: {other:?}"),
    }
}

#[test]
fn qualified_module_paths() {
    let n = main_u32(
        r#"
mod geometry {
  pub enum Shape {
    Circle(U32)
    Square(U32)
  }

  pub fn area(s: Shape): U32 {
    match s {
      Circle(r) => r * r * 3u32
      Square(w) => w * w
    }
  }
}

fn main(): U32 {
  geometry::area(geometry::Shape::Circle(2u32))
}
"#,
    );
    assert_eq!(n, 12);
}

#[test]
fn use_imports_its_targets() {
    let n = main_u32(
        r#"
mod geometry {
  pub enum Shape {
    Circle(U32)
    Square(U32)
  }
}

use geometry::Shape
use geometry::Shape::{Circle, Square}

fn side(s: Shape): U32 {
  match s {
    Circle(r) => r
    Square(w) => w
  }
}

fn main(): U32 {
  side(Circle(7u32))
}
"#,
    );
    assert_eq!(n, 7);
}

#[test]
fn aliases_and_globs() {
    let n = main_u32(
        r#"
mod geometry {
  pub enum Shape {
    Circle(U32)
  }
}

use geometry::Shape as S
use geometry::Shape::*

fn side(s: S): U32 {
  match s {
    Circle(r) => r
  }
}

fn main(): U32 {
  side(Circle(4u32))
}
"#,
    );
    assert_eq!(n, 4);
}

#[test]
fn self_and_super_paths() {
    let n = main_u32(
        r#"
mod outer {
  pub fn base(): U32 { 1u32 }

  pub mod inner {
    pub fn next(): U32 { super::base() + 1u32 }
    pub fn own(): U32 { self::next() }
  }
}

fn main(): U32 {
  outer::inner::own()
}
"#,
    );
    assert_eq!(n, 2);
}

#[test]
fn use_is_root_relative_unless_qualified() {
    let n = main_u32(
        r#"
mod outer {
  pub mod inner {
    pub fn one(): U32 { 1u32 }
  }

  use self::inner::one

  pub fn two(): U32 { one() + 1u32 }
}

fn main(): U32 {
  outer::two()
}
"#,
    );
    assert_eq!(n, 2);
}

#[test]
fn private_items_are_module_local() {
    assert_rejected(
        r#"
mod secret {
  fn answer(): U32 { 42u32 }
}

fn main(): U32 {
  secret::answer()
}
"#,
        "is private here",
    );
    assert_rejected(
        r#"
mod outer {
  mod inner {
    pub fn answer(): U32 { 42u32 }
  }
}

fn main(): U32 {
  outer::inner::answer()
}
"#,
        "module `outer::inner` is private here",
    );
}

#[test]
fn a_public_module_is_reachable() {
    let n = main_u32(
        r#"
mod outer {
  pub mod inner {
    pub fn answer(): U32 { 42u32 }
  }
}

fn main(): U32 {
  outer::inner::answer()
}
"#,
    );
    assert_eq!(n, 42);
}

#[test]
fn methods_are_private_by_default() {
    let n = main_u32(
        r#"
mod shapes {
  pub enum Shape {
    Dot
  }

  impl Shape {
    pub fn area(self: Shape): U32 { 0u32 }
    fn hidden(self: Shape): U32 { 1u32 }
  }

  pub fn public_area(s: Shape): U32 { s.area() }
}

fn main(): U32 {
  shapes::public_area(shapes::Shape::Dot)
}
"#,
    );
    assert_eq!(n, 0);
    assert_rejected(
        r#"
mod shapes {
  pub enum Shape {
    Dot
  }

  impl Shape {
    fn hidden(self: Shape): U32 { 1u32 }
  }
}

fn main(): U32 {
  shapes::Shape::Dot.hidden()
}
"#,
        "is private here",
    );
}

#[test]
fn prelude_types_and_constructors_are_auto_imported() {
    let n = main_u32("fn main(): U32 { 2u32 }");
    assert_eq!(n, 2);
    let globals = elaborate(
        r#"
fn main(): List[Nat] {
  Cons(1, Cons(0, Nil))
}
"#,
    );
    assert!(globals.get_ty("main").is_some());
}

#[test]
fn prelude_values_need_use() {
    assert_rejected(
        r#"
fn main(): Nat {
  nat_add(Zero, Zero)
}
"#,
        "unknown name `nat_add`",
    );
    let globals = elaborate(
        r#"
use nat_add

fn main(): Nat {
  nat_add(Zero, Zero)
}
"#,
    );
    assert!(globals.get_ty("main").is_some());
}

#[test]
fn duplicate_and_re_exported_modules_are_rejected() {
    assert_rejected(
        r#"
mod a {}
mod a {}
"#,
        "already defined in this module",
    );
    assert_rejected(
        r#"
mod a {
  pub fn f(): U32 { 0u32 }
}
pub use a::f
"#,
        "re-exports are not supported",
    );
}

#[test]
fn a_module_cannot_shadow_a_prelude_module() {
    assert_rejected(
        r#"
mod stderr {
  pub fn write(): U32 { 0u32 }
}
"#,
        "already defined",
    );
}

#[test]
fn module_names_do_not_share_a_value_namespace() {
    assert_rejected(
        r#"
mod a {}

fn main(): U32 {
  a
}
"#,
        "is a module, not a value",
    );
}
