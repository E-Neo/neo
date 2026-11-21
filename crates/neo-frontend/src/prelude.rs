use crate::elab::elaborate_prelude;
use crate::env::Globals;
use neo_common::error::Result;

/// The Stage 0 prelude: the inductive types every program starts with.
pub const PRELUDE: &str = r#"
pub enum Nat {
  Zero
  Succ(Nat)
}

pub enum Bool {
  False
  True
}

pub enum Unit {
  Unit
}

pub enum List[T: Type] {
  Nil
  Cons(T, List[T])
}

pub enum False : Prop {
}

pub enum True : Prop {
  Trivial
}

pub enum And[P: Prop, Q: Prop] : Prop {
  Pair(P, Q)
}

pub enum Or[P: Prop, Q: Prop] : Prop {
  Left(P)
  Right(Q)
}

pub enum Exists[T: Type, P: fn(x: T) -> Prop] : Prop {
  Witness(x: T, P(x))
}

pub enum Option[T: Type] {
  None
  Some(T)
}

pub enum Result[T: Type, E: Type] {
  Ok(T)
  Err(E)
}

pub enum Utf8Error {
  Invalid(U32)
}

pub enum IoError {
  Closed
  Failed
}

pub fn bool_not(b: Bool): Bool {
  match b {
    True => False
    False => True
  }
}

pub fn bool_eq(a: Bool, b: Bool): Bool {
  match a {
    True => b
    False => bool_not(b)
  }
}

pub fn bool_ne(a: Bool, b: Bool): Bool {
  bool_not(bool_eq(a, b))
}

pub fn nat_add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(nat_add(p, b))
  }
}

pub fn nat_sub(a: Nat, b: Nat): Nat {
  match a {
    Zero => Zero
    Succ(p) => match b {
      Zero => Succ(p)
      Succ(q) => nat_sub(p, q)
    }
  }
}

pub fn nat_mul(a: Nat, b: Nat): Nat {
  match a {
    Zero => Zero
    Succ(p) => nat_add(b, nat_mul(p, b))
  }
}

pub fn nat_eq(a: Nat, b: Nat): Bool {
  match a {
    Zero => match b {
      Zero => True
      Succ(_) => False
    }
    Succ(p) => match b {
      Zero => False
      Succ(q) => nat_eq(p, q)
    }
  }
}

pub fn nat_ne(a: Nat, b: Nat): Bool {
  bool_not(nat_eq(a, b))
}

pub fn nat_lt(a: Nat, b: Nat): Bool {
  match a {
    Zero => match b {
      Zero => False
      Succ(_) => True
    }
    Succ(p) => match b {
      Zero => False
      Succ(q) => nat_lt(p, q)
    }
  }
}

pub fn nat_le(a: Nat, b: Nat): Bool {
  match a {
    Zero => True
    Succ(p) => match b {
      Zero => False
      Succ(q) => nat_le(p, q)
    }
  }
}

pub fn nat_gt(a: Nat, b: Nat): Bool {
  nat_lt(b, a)
}

pub fn nat_ge(a: Nat, b: Nat): Bool {
  nat_le(b, a)
}

pub fn nat_div2(a: Nat): Nat {
  match a {
    Zero => Zero
    Succ(Zero) => Zero
    Succ(Succ(p)) => Succ(nat_div2(p))
  }
}

pub fn nat_mod2(a: Nat): Nat {
  match a {
    Zero => Zero
    Succ(Zero) => Succ(Zero)
    Succ(Succ(p)) => nat_mod2(p)
  }
}

pub fn nat_and_bit(x: Nat, y: Nat): Nat {
  match x {
    Zero => Zero
    Succ(_) => match y {
      Zero => Zero
      Succ(_) => Succ(Zero)
    }
  }
}

pub fn nat_or_bit(x: Nat, y: Nat): Nat {
  match x {
    Zero => y
    Succ(_) => Succ(Zero)
  }
}

pub fn nat_xor_bit(x: Nat, y: Nat): Nat {
  match x {
    Zero => y
    Succ(_) => match y {
      Zero => Succ(Zero)
      Succ(_) => Zero
    }
  }
}

pub fn nat_add_if(bit: Nat, x: Nat, acc: Nat): Nat {
  match bit {
    Zero => acc
    Succ(_) => nat_add(x, acc)
  }
}

pub fn nat_both_zero(a: Nat, b: Nat): Bool {
  match a {
    Zero => match b {
      Zero => True
      Succ(_) => False
    }
    Succ(_) => False
  }
}

pub fn nat_bitwise(
  f: fn(Nat, Nat) -> Nat,
  fuel: Nat,
  a: Nat,
  b: Nat,
  pow: Nat,
  acc: Nat
): Nat {
  match fuel {
    Zero => acc
    Succ(g) => match nat_both_zero(a, b) {
      True => acc
      False => nat_bitwise(
        f,
        g,
        nat_div2(a),
        nat_div2(b),
        nat_mul(pow, 2),
        nat_add_if(f(nat_mod2(a), nat_mod2(b)), pow, acc)
      )
    }
  }
}

pub fn nat_and(a: Nat, b: Nat): Nat {
  nat_bitwise(nat_and_bit, nat_add(a, b), a, b, 1, 0)
}

pub fn nat_or(a: Nat, b: Nat): Nat {
  nat_bitwise(nat_or_bit, nat_add(a, b), a, b, 1, 0)
}

pub fn nat_xor(a: Nat, b: Nat): Nat {
  nat_bitwise(nat_xor_bit, nat_add(a, b), a, b, 1, 0)
}

pub fn nat_shl(a: Nat, b: Nat): Nat {
  match b {
    Zero => a
    Succ(p) => match a {
      Zero => Zero
      Succ(_) => nat_shl(nat_mul(a, 2), p)
    }
  }
}

pub fn nat_shr(a: Nat, b: Nat): Nat {
  match b {
    Zero => a
    Succ(p) => match a {
      Zero => Zero
      Succ(_) => nat_shr(nat_div2(a), p)
    }
  }
}

pub fn nat_div_aux(a: Nat, b: Nat, q: Nat, r: Nat): Nat {
  match a {
    Zero => q
    Succ(p) => match b {
      Zero => Zero
      Succ(bp) => match nat_eq(r, bp) {
        True => nat_div_aux(p, b, Succ(q), Zero)
        False => nat_div_aux(p, b, q, Succ(r))
      }
    }
  }
}

pub fn nat_div(a: Nat, b: Nat): Nat {
  nat_div_aux(a, b, Zero, Zero)
}

pub fn nat_rem_aux(a: Nat, b: Nat, r: Nat): Nat {
  match a {
    Zero => r
    Succ(p) => match b {
      Zero => Zero
      Succ(bp) => match nat_eq(r, bp) {
        True => nat_rem_aux(p, b, Zero)
        False => nat_rem_aux(p, b, Succ(r))
      }
    }
  }
}

pub fn nat_rem(a: Nat, b: Nat): Nat {
  nat_rem_aux(a, b, Zero)
}

pub fn unit_eq(a: Unit, b: Unit): Bool {
  True
}

pub fn unit_ne(a: Unit, b: Unit): Bool {
  False
}

pub mod stderr {
  pub extern fn write(s: String): IO[super::Result[super::Unit, super::IoError]] = "wasi:cli/stderr@0.2.9#write"
}

impl String {
  pub fn length(self: String): U32 {
    str_length(self)
  }

  pub fn slice(self: String, start: U32, end: U32): String {
    str_slice(self, start, end)
  }

  pub fn concat(self: String, other: String): String {
    str_concat(self, other)
  }

  pub fn to_bytes(self: String): Bytes {
    str_to_bytes(self)
  }
}

impl Bytes {
  pub fn length(self: Bytes): U32 {
    bytes_length(self)
  }

  pub fn slice(self: Bytes, start: U32, end: U32): Bytes {
    bytes_slice(self, start, end)
  }

  pub fn concat(self: Bytes, other: Bytes): Bytes {
    bytes_concat(self, other)
  }

  pub fn get(self: Bytes, index: U32): U32 {
    bytes_get(self, index)
  }

  pub fn to_str(self: Bytes): Result[String, Utf8Error] {
    let code = bytes_utf8_check(self)
    match code == 0u32 {
      True => Ok(str_from_bytes(self))
      False => Err(Invalid(code - 1u32))
    }
  }
}

"#;

/// Install the prelude declarations into a fresh global environment.
pub fn install(globals: &mut Globals) -> Result<()> {
    let program = crate::parser::parse_source(PRELUDE)?;
    elaborate_prelude(globals, &program)
}

pub fn new_globals() -> Result<Globals> {
    let mut globals = Globals::new();
    install(&mut globals)?;
    Ok(globals)
}
