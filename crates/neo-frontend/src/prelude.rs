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

enum List[T: Type] {
  nil
  cons(T, List[T])
}

enum False : Prop {
}

enum True : Prop {
  trivial
}

enum And[P: Prop, Q: Prop] : Prop {
  and(P, Q)
}

enum Or[P: Prop, Q: Prop] : Prop {
  or_left(P)
  or_right(Q)
}

enum Exists[T: Type, P: fn(x: T) -> Prop] : Prop {
  exists(x: T, P(x))
}

fn bool_not(b: Bool): Bool {
  match b {
    true => false
    false => true
  }
}

fn bool_eq(a: Bool, b: Bool): Bool {
  match a {
    true => b
    false => bool_not(b)
  }
}

fn bool_ne(a: Bool, b: Bool): Bool {
  bool_not(bool_eq(a, b))
}

fn nat_add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(nat_add(p, b))
  }
}

fn nat_sub(a: Nat, b: Nat): Nat {
  match a {
    zero => zero
    succ(p) => match b {
      zero => succ(p)
      succ(q) => nat_sub(p, q)
    }
  }
}

fn nat_mul(a: Nat, b: Nat): Nat {
  match a {
    zero => zero
    succ(p) => nat_add(b, nat_mul(p, b))
  }
}

fn nat_eq(a: Nat, b: Nat): Bool {
  match a {
    zero => match b {
      zero => true
      succ(_) => false
    }
    succ(p) => match b {
      zero => false
      succ(q) => nat_eq(p, q)
    }
  }
}

fn nat_ne(a: Nat, b: Nat): Bool {
  bool_not(nat_eq(a, b))
}

fn nat_lt(a: Nat, b: Nat): Bool {
  match a {
    zero => match b {
      zero => false
      succ(_) => true
    }
    succ(p) => match b {
      zero => false
      succ(q) => nat_lt(p, q)
    }
  }
}

fn nat_le(a: Nat, b: Nat): Bool {
  match a {
    zero => true
    succ(p) => match b {
      zero => false
      succ(q) => nat_le(p, q)
    }
  }
}

fn nat_gt(a: Nat, b: Nat): Bool {
  nat_lt(b, a)
}

fn nat_ge(a: Nat, b: Nat): Bool {
  nat_le(b, a)
}

fn nat_div2(a: Nat): Nat {
  match a {
    zero => zero
    succ(zero) => zero
    succ(succ(p)) => succ(nat_div2(p))
  }
}

fn nat_mod2(a: Nat): Nat {
  match a {
    zero => zero
    succ(zero) => succ(zero)
    succ(succ(p)) => nat_mod2(p)
  }
}

fn nat_and_bit(x: Nat, y: Nat): Nat {
  match x {
    zero => zero
    succ(_) => match y {
      zero => zero
      succ(_) => succ(zero)
    }
  }
}

fn nat_or_bit(x: Nat, y: Nat): Nat {
  match x {
    zero => y
    succ(_) => succ(zero)
  }
}

fn nat_xor_bit(x: Nat, y: Nat): Nat {
  match x {
    zero => y
    succ(_) => match y {
      zero => succ(zero)
      succ(_) => zero
    }
  }
}

fn nat_add_if(bit: Nat, x: Nat, acc: Nat): Nat {
  match bit {
    zero => acc
    succ(_) => nat_add(x, acc)
  }
}

fn nat_both_zero(a: Nat, b: Nat): Bool {
  match a {
    zero => match b {
      zero => true
      succ(_) => false
    }
    succ(_) => false
  }
}

fn nat_bitwise(
  f: fn(Nat, Nat) -> Nat,
  fuel: Nat,
  a: Nat,
  b: Nat,
  pow: Nat,
  acc: Nat
): Nat {
  match fuel {
    zero => acc
    succ(g) => match nat_both_zero(a, b) {
      true => acc
      false => nat_bitwise(
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

fn nat_and(a: Nat, b: Nat): Nat {
  nat_bitwise(nat_and_bit, nat_add(a, b), a, b, 1, 0)
}

fn nat_or(a: Nat, b: Nat): Nat {
  nat_bitwise(nat_or_bit, nat_add(a, b), a, b, 1, 0)
}

fn nat_xor(a: Nat, b: Nat): Nat {
  nat_bitwise(nat_xor_bit, nat_add(a, b), a, b, 1, 0)
}

fn nat_shl(a: Nat, b: Nat): Nat {
  match b {
    zero => a
    succ(p) => match a {
      zero => zero
      succ(_) => nat_shl(nat_mul(a, 2), p)
    }
  }
}

fn nat_shr(a: Nat, b: Nat): Nat {
  match b {
    zero => a
    succ(p) => match a {
      zero => zero
      succ(_) => nat_shr(nat_div2(a), p)
    }
  }
}

fn nat_div_aux(a: Nat, b: Nat, q: Nat, r: Nat): Nat {
  match a {
    zero => q
    succ(p) => match b {
      zero => zero
      succ(bp) => match nat_eq(r, bp) {
        true => nat_div_aux(p, b, succ(q), zero)
        false => nat_div_aux(p, b, q, succ(r))
      }
    }
  }
}

fn nat_div(a: Nat, b: Nat): Nat {
  nat_div_aux(a, b, zero, zero)
}

fn nat_rem_aux(a: Nat, b: Nat, r: Nat): Nat {
  match a {
    zero => r
    succ(p) => match b {
      zero => zero
      succ(bp) => match nat_eq(r, bp) {
        true => nat_rem_aux(p, b, zero)
        false => nat_rem_aux(p, b, succ(r))
      }
    }
  }
}

fn nat_rem(a: Nat, b: Nat): Nat {
  nat_rem_aux(a, b, zero)
}

fn unit_eq(a: Unit, b: Unit): Bool {
  true
}

fn unit_ne(a: Unit, b: Unit): Bool {
  false
}

fn String.length(self: String): U32 {
  str_length(self)
}

fn String.slice(self: String, start: U32, end: U32): String {
  str_slice(self, start, end)
}

fn String.concat(self: String, other: String): String {
  str_concat(self, other)
}

fn String.to_bytes(self: String): Bytes {
  str_to_bytes(self)
}

fn Bytes.length(self: Bytes): U32 {
  bytes_length(self)
}

fn Bytes.slice(self: Bytes, start: U32, end: U32): Bytes {
  bytes_slice(self, start, end)
}

fn Bytes.concat(self: Bytes, other: Bytes): Bytes {
  bytes_concat(self, other)
}

fn Bytes.get(self: Bytes, index: U32): U32 {
  bytes_get(self, index)
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
