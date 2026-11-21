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
    match parse_source(src) {
        Err(e) => e,
        Ok(ast) => elaborate_program(&mut globals, &ast).unwrap_err(),
    }
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
        let next = force(globals, current);
        if matches!(&*next, Val::Glob(n) if *n == name) {
            return next;
        }
        current = next;
    }
}

fn utf8_check(literal: &str) -> u32 {
    let globals = elaborate(&format!(
        "fn main(): Result[String, Utf8Error] {{ {literal}.to_str() }}"
    ));
    match &*main_value(&globals) {
        Val::Ctor(name, 0, _) if name == "Result" => 0,
        Val::Ctor(name, _, args) if name == "Result" => {
            let fields = ctor_fields(&globals, "Result", 1, args);
            match &*fields[0] {
                Val::Ctor(name, 0, args) if name == "Utf8Error" => {
                    let fields = ctor_fields(&globals, "Utf8Error", 0, args);
                    match &*fields[0] {
                        Val::PrimLit(PrimLit::U32(n)) => *n + 1,
                        other => panic!("`Invalid` did not carry a u32: {other:?}"),
                    }
                }
                other => panic!("`Err` did not carry a Utf8Error: {other:?}"),
            }
        }
        other => panic!("`main` did not evaluate to a Result: {other:?}"),
    }
}

/// The fields of a constructor value, skipping its type parameters.
fn ctor_fields<'a>(globals: &Globals, enum_name: &str, index: usize, args: &'a [V]) -> &'a [V] {
    let ctor = &globals.get_enum(enum_name).expect("enum").ctors[index];
    &args[ctor.num_params..]
}

fn to_str(expr: &str) -> Result<Vec<u8>, u32> {
    let globals = elaborate(&format!(
        "fn main(): Result[String, Utf8Error] {{ {expr}.to_str() }}"
    ));
    match &*main_value(&globals) {
        Val::Ctor(name, index, args) if name == "Result" => {
            let fields = ctor_fields(&globals, "Result", *index, args);
            if *index == 0 {
                match &*fields[0] {
                    Val::PrimLit(PrimLit::Str(bytes)) => Ok(bytes.to_vec()),
                    other => panic!("`ok` did not carry a string: {other:?}"),
                }
            } else {
                match &*fields[0] {
                    Val::Ctor(name, index, args) if name == "Utf8Error" && *index == 0 => {
                        let fields = ctor_fields(&globals, "Utf8Error", 0, args);
                        match &*fields[0] {
                            Val::PrimLit(PrimLit::U32(n)) => Err(*n),
                            other => panic!("`invalid_utf8` did not carry a u32: {other:?}"),
                        }
                    }
                    other => panic!("`err` did not carry a Utf8Error: {other:?}"),
                }
            }
        }
        other => panic!("`main` did not evaluate to a Result: {other:?}"),
    }
}

#[test]
fn accepts_valid_utf8() {
    assert_eq!(utf8_check(r#"b"""#), 0);
    assert_eq!(utf8_check(r#"b"abc""#), 0);
    assert_eq!(utf8_check(r#"b"\x00""#), 0);
    // Smallest and largest two-byte sequences.
    assert_eq!(utf8_check(r#"b"\xc2\x80""#), 0);
    assert_eq!(utf8_check(r#"b"\xdf\xbf""#), 0);
    // Three-byte sequences, including the surrogate-adjacent boundaries.
    assert_eq!(utf8_check(r#"b"\xe0\xa0\x80""#), 0);
    assert_eq!(utf8_check(r#"b"\xed\x9f\xbf""#), 0);
    assert_eq!(utf8_check(r#"b"\xee\x80\x80""#), 0);
    assert_eq!(utf8_check(r#"b"\xe2\x82\xac""#), 0);
    // Four-byte sequences, including the maximum code point.
    assert_eq!(utf8_check(r#"b"\xf0\x90\x80\x80""#), 0);
    assert_eq!(utf8_check(r#"b"\xf4\x8f\xbf\xbf""#), 0);
    assert_eq!(utf8_check(r#"b"\xf0\x9f\x98\x80""#), 0);
}

#[test]
fn rejects_invalid_lead_bytes() {
    assert_eq!(utf8_check(r#"b"\x80""#), 1);
    assert_eq!(utf8_check(r#"b"\xbf""#), 1);
    assert_eq!(utf8_check(r#"b"\xc0\xaf""#), 1);
    assert_eq!(utf8_check(r#"b"\xc1\xbf""#), 1);
    assert_eq!(utf8_check(r#"b"\xf5\x80\x80\x80""#), 1);
    assert_eq!(utf8_check(r#"b"\xff""#), 1);
}

#[test]
fn rejects_overlong_forms() {
    // 3-byte encoding of a value that fits in 2 bytes.
    assert_eq!(utf8_check(r#"b"\xe0\x80\x80""#), 1);
    // 4-byte encoding of a value that fits in 3 bytes.
    assert_eq!(utf8_check(r#"b"\xf0\x8f\xbf\xbf""#), 1);
}

#[test]
fn rejects_surrogates_and_out_of_range_codepoints() {
    assert_eq!(utf8_check(r#"b"\xed\xa0\x80""#), 1);
    assert_eq!(utf8_check(r#"b"\xed\xbf\xbf""#), 1);
    assert_eq!(utf8_check(r#"b"\xf4\x90\x80\x80""#), 1);
}

#[test]
fn rejects_bad_continuation_bytes() {
    assert_eq!(utf8_check(r#"b"\xe2\x41\xac""#), 1);
    assert_eq!(utf8_check(r#"b"\xe2\x82\x41""#), 1);
    assert_eq!(utf8_check(r#"b"\xf0\x9f\x98\x41""#), 1);
    // A stray continuation byte after a valid sequence.
    assert_eq!(utf8_check(r#"b"a\x80""#), 2);
}

#[test]
fn reports_the_start_of_the_first_bad_sequence() {
    assert_eq!(utf8_check(r#"b"ab\xff""#), 3);
    assert_eq!(utf8_check(r#"b"ab\xe2\x82""#), 3);
    assert_eq!(utf8_check(r#"b"ab\xe2\x82\xac\xff""#), 6);
}

#[test]
fn rejects_truncated_sequences() {
    assert_eq!(utf8_check(r#"b"\xc2""#), 1);
    assert_eq!(utf8_check(r#"b"\xe2\x82""#), 1);
    assert_eq!(utf8_check(r#"b"\xf0\x9f\x98""#), 1);
    assert_eq!(utf8_check(r#"b"a\xe2""#), 2);
}

#[test]
fn to_str_returns_the_same_bytes() {
    assert_eq!(to_str(r#"b"abc""#), Ok(b"abc".to_vec()));
    assert_eq!(to_str(r#"b"""#), Ok(Vec::new()));
    assert_eq!(to_str(r#"b"a\x00b""#), Ok(b"a\x00b".to_vec()));
    assert_eq!(
        to_str(r#"b"\xe2\x82\xac\xf0\x9f\x98\x80""#),
        Ok("\u{20ac}\u{1f600}".as_bytes().to_vec())
    );
}

#[test]
fn to_str_reports_the_first_invalid_byte() {
    assert_eq!(to_str(r#"b"\xff""#), Err(0));
    assert_eq!(to_str(r#"b"a\xff""#), Err(1));
    assert_eq!(to_str(r#"b"abc\xed\xa0\x80""#), Err(3));
    assert_eq!(to_str(r#"b"Ok\xe2\x82""#), Err(2));
}

#[test]
fn validation_uses_the_slice_bounds() {
    assert_eq!(to_str(r#"b"abc".slice(1, 3)"#), Ok(b"bc".to_vec()));
    assert_eq!(to_str(r#"b"xx\xff".slice(1, 3)"#), Err(1));
    assert_eq!(utf8_check(r#"b"xx\xff".slice(1, 3)"#), 2);
    assert_eq!(
        to_str(r#"b"xx\xe2\x82\xac".slice(2, 5)"#),
        Ok("\u{20ac}".as_bytes().to_vec())
    );
}

#[test]
fn to_str_is_the_inverse_of_to_bytes() {
    let globals = elaborate(
        r#"
fn main(): Bool {
  match "neo".to_bytes().to_str() {
    Ok(text) => text == "neo"
    Err(_) => Bool::False
  }
}
"#,
    );
    match &*main_value(&globals) {
        Val::Ctor(name, index, _) if name == "Bool" => assert_eq!(*index, 1),
        other => panic!("`main` did not evaluate to a bool: {other:?}"),
    }
}

#[test]
fn rejects_wrong_argument_types() {
    assert_rejected(
        r#"fn main(): U32 { bytes_utf8_check("abc") }"#,
        "low-level primitive",
    );
    assert_rejected(
        r#"fn main(): String { str_from_bytes("abc") }"#,
        "low-level primitive",
    );
}

#[test]
fn option_and_result_are_printable_prelude_types() {
    let globals = elaborate(
        r#"
fn main(): Option[Nat] {
  Some(3)
}
"#,
    );
    match &*main_value(&globals) {
        Val::Ctor(name, index, _) if name == "Option" => assert_eq!(*index, 1),
        other => panic!("`main` did not evaluate to an Option: {other:?}"),
    }
}
