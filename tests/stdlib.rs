//! Tests for the Neo-written standard library: `Ordering`, list helpers, and `Map`.

#![cfg(not(target_family = "wasm"))]

use neo::elab::elaborate_program;
use neo::env::Globals;
use neo::eval::force;
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::value::{V, Val};

fn define(src: &str) -> Globals {
    let mut globals = new_globals().expect("prelude");
    let ast = parse_source(src).expect("parse");
    elaborate_program(&mut globals, &ast).expect("elaborate");
    globals
}

fn call(globals: &Globals, name: &str, args: &[V]) -> V {
    let mut value: V = std::rc::Rc::new(Val::Glob(name.to_string()));
    for arg in args {
        value = neo::eval::vapp(globals, value, arg.clone());
    }
    force(globals, value)
}

fn as_nat(globals: &Globals, value: &V) -> u64 {
    let info = globals.get_enum("Nat").expect("Nat");
    let zero = info.ctors.iter().position(|c| c.name == "Zero").unwrap();
    let succ = info.ctors.iter().position(|c| c.name == "Succ").unwrap();
    fn go(value: &V, zero: usize, succ: usize) -> u64 {
        match &**value {
            Val::Ctor(name, index, _) if name == "Nat" && *index == zero => 0,
            Val::Ctor(name, index, args) if name == "Nat" && *index == succ => {
                1 + go(&args[0], zero, succ)
            }
            other => panic!("not a Nat: {other:?}"),
        }
    }
    go(value, zero, succ)
}

fn nat_main(src: &str) -> u64 {
    let globals = define(src);
    as_nat(&globals, &call(&globals, "main", &[]))
}

#[test]
fn ordering_selects_all_three_cases() {
    assert_eq!(
        nat_main(
            r#"
fn main(): Nat {
  match (2 : Nat).cmp(3) {
    Less => 0
    Equal => 1
    Greater => 2
  }
}
"#
        ),
        0
    );
    assert_eq!(
        nat_main(
            r#"
fn main(): Nat {
  match (3 : Nat).cmp(3) {
    Less => 0
    Equal => 1
    Greater => 2
  }
}
"#
        ),
        1
    );
    assert_eq!(
        nat_main(
            r#"
fn main(): Nat {
  match (4 : Nat).cmp(3) {
    Less => 0
    Equal => 1
    Greater => 2
  }
}
"#
        ),
        2
    );
}

#[test]
fn list_helpers() {
    assert_eq!(
        nat_main(
            r#"
fn main(): Nat {
  let xs: List[Nat] = Cons(1, Cons(2, Cons(3, Nil)))
  xs.len()
}
"#
        ),
        3
    );
    assert_eq!(
        nat_main(
            r#"
fn main(): Nat {
  let xs: List[Nat] = Cons(1, Cons(2, Nil))
  let ys: List[Nat] = Cons(3, Nil)
  xs.append(ys).len()
}
"#
        ),
        3
    );
    assert_eq!(
        nat_main(
            r#"
fn main(): Nat {
  let xs: List[Nat] = Cons(1, Cons(2, Nil))
  match xs.reverse() {
    Nil => 0
    Cons(x, _) => x
  }
}
"#
        ),
        2
    );
    let globals = define(
        r#"
fn main(): Bool {
  let xs: List[Nat] = Nil
  xs.is_empty()
}
"#,
    );
    assert!(matches!(
        &*call(&globals, "main", &[]),
        Val::Ctor(name, 1, _) if name == "Bool"
    ));
}

#[test]
fn string_is_empty() {
    assert_eq!(
        nat_main(
            r#"
fn main(): Nat {
  match "".is_empty() {
    True => 1
    False => 0
  }
}
"#
        ),
        1
    );
    assert_eq!(
        nat_main(
            r#"
fn main(): Nat {
  match "x".is_empty() {
    True => 1
    False => 0
  }
}
"#
        ),
        0
    );
}

#[test]
fn map_insert_get_size_and_update() {
    assert_eq!(
        nat_main(
            r#"
use map_empty
use nat_cmp

fn main(): Nat {
  let m: Map[Nat, Nat] = map_empty(nat_cmp)
  let m = m.insert(3, 30)
  let m = m.insert(1, 10)
  let m = m.insert(2, 20)
  match m.get(2) {
    Some(v) => v
    None => 0
  }
}
"#
        ),
        20
    );
    assert_eq!(
        nat_main(
            r#"
use map_empty
use nat_cmp

fn main(): Nat {
  let m: Map[Nat, Nat] = map_empty(nat_cmp)
  let m = m.insert(1, 10)
  let m = m.insert(2, 20)
  m.size()
}
"#
        ),
        2
    );
    assert_eq!(
        nat_main(
            r#"
use map_empty
use nat_cmp

fn main(): Nat {
  let m: Map[Nat, Nat] = map_empty(nat_cmp)
  let m = m.insert(1, 10)
  let m = m.insert(1, 11)
  match m.get(1) {
    Some(v) => v
    None => 0
  }
}
"#
        ),
        11
    );
}

#[test]
fn map_missing_key_is_none() {
    let globals = define(
        r#"
use map_empty
use nat_cmp

fn main(): Option[Nat] {
  let m: Map[Nat, Nat] = map_empty(nat_cmp)
  m.get(9)
}
"#,
    );
    match &*call(&globals, "main", &[]) {
        Val::Ctor(name, index, _) => {
            assert_eq!(name, "Option");
            assert_eq!(*index, 0);
        }
        other => panic!("not an Option: {other:?}"),
    }
}

#[test]
fn map_is_printable_when_its_fields_are() {
    let globals = define(
        r#"
use map_empty
use nat_cmp

fn main(): Nat {
  let m: Map[Nat, Nat] = map_empty(nat_cmp)
  let m = m.insert(1, 10)
  match m {
    Map::New(_, tree) => match tree {
      Tip => 0
      Bin(_, _, _, _, _) => 1
    }
  }
}
"#,
    );
    assert_eq!(as_nat(&globals, &call(&globals, "main", &[])), 1);
}

#[test]
fn map_stays_balanced_under_sorted_inserts() {
    assert_eq!(
        nat_main(
            r#"
use map_empty
use nat_cmp

fn main(): Nat {
  let m: Map[Nat, Nat] = map_empty(nat_cmp)
  let m = m.insert(1, 0)
  let m = m.insert(2, 0)
  let m = m.insert(3, 0)
  let m = m.insert(4, 0)
  let m = m.insert(5, 0)
  let m = m.insert(6, 0)
  let m = m.insert(7, 0)
  match m {
    Map::New(_, tree) => match tree {
      Tip => 0
      Bin(h, _, _, _, _) => h
    }
  }
}
"#
        ),
        3
    );
}
