use crate::elab::elaborate_program;
use crate::env::Globals;
use neo_common::error::Result;

/// The Stage 0 prelude: the inductive types every program starts with.
pub const PRELUDE: &str = r#"
enum Nat {
  zero
  succ(Nat)
}

enum Bool {
  false
  true
}

enum Unit {
  unit
}

enum String {
  str_nil
  str_cons(Nat, String)
}

enum List(T: Type) {
  nil
  cons(T, List(T))
}

enum False : Prop {
}

enum True : Prop {
  trivial
}

enum And(P: Prop, Q: Prop) : Prop {
  and(P, Q)
}

enum Or(P: Prop, Q: Prop) : Prop {
  or_left(P)
  or_right(Q)
}

enum Exists(T: Type, P: fn(x: T) -> Prop) : Prop {
  exists(x: T, P(x))
}

fn str_length(s: String): Nat {
  match s {
    str_nil => zero
    str_cons(byte, rest) => succ(str_length(rest))
  }
}

fn str_concat(a: String, b: String): String {
  match a {
    str_nil => b
    str_cons(byte, rest) => str_cons(byte, str_concat(rest, b))
  }
}

fn str_byte_eq(a: Nat, b: Nat): Bool {
  match a {
    zero => match b {
      zero => true
      succ(k) => false
    }
    succ(k) => match b {
      zero => false
      succ(j) => str_byte_eq(k, j)
    }
  }
}

fn str_eq(a: String, b: String): Bool {
  match a {
    str_nil => match b {
      str_nil => true
      str_cons(byte, rest) => false
    }
    str_cons(byte, rest) => match b {
      str_nil => false
      str_cons(other, others) => match str_byte_eq(byte, other) {
        true => str_eq(rest, others)
        false => false
      }
    }
  }
}
"#;

/// Install the prelude declarations into a fresh global environment.
pub fn install(globals: &mut Globals) -> Result<()> {
    let program = crate::parser::parse_source(PRELUDE)?;
    elaborate_program(globals, &program)
}

pub fn new_globals() -> Result<Globals> {
    let mut globals = Globals::new();
    install(&mut globals)?;
    Ok(globals)
}
