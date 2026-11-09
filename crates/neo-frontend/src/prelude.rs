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
