# The Neo Language Reference

Neo is a dependently typed, total programming language with a small proof
kernel. Programs use an ASCII, math-like syntax and compile to WebAssembly
components that run on WASI 0.2.

This document describes **Stage 0**, the Rust implementation in this
repository. Everything here is checked against that implementation. Features
that are planned but not implemented are listed in
[Limitations](#17-limitations-and-not-yet-supported).

---

## Contents

1. [Quick start](#1-quick-start)
2. [Command line](#2-command-line)
3. [Lexical structure](#3-lexical-structure)
4. [Declarations](#4-declarations)
5. [Types and universes](#5-types-and-universes)
6. [Enums](#6-enums)
7. [Functions and expressions](#7-functions-and-expressions)
8. [Pattern matching](#8-pattern-matching)
9. [Equality and proofs](#9-equality-and-proofs)
10. [Totality and termination](#10-totality-and-termination)
11. [Positivity](#11-positivity)
12. [The prelude](#12-the-prelude)
13. [Printing and building for WASI](#13-printing-and-building-for-wasi)
14. [The REPL](#14-the-repl)
15. [Grammar](#15-grammar)
16. [Errors](#16-errors)
17. [Limitations and not-yet-supported](#17-limitations-and-not-yet-supported)
18. [Worked examples](#18-worked-examples)
19. [Coming from other languages](#19-coming-from-other-languages)

---

## 1. Quick start

The smallest Neo program:

```neo
fn main(): IO[Nat] {
  IO::pure(5)
}
```

Save it as `hello.neo`, then:

```sh
neo hello.neo                      # interpret: prints 5
neo build hello.neo -o hello.wasm  # compile to a WASI 0.2 component
wasmtime hello.wasm                # runs the component: prints 5
```

`neo build` writes `hello.wasm` next to the input by default.

`main` produces an `IO` action. `IO::pure(v)` is an action that produces `v`;
`IO::bind(m, k)` runs `m` and passes its result to the function `k`. Host
operations are declared with `extern`, such as `stderr::write`. Neo runs
`main`'s action once and prints its result, including `String` results.

---

## 2. Command line

```
neo                                  start the REPL
neo <file.neo>                       interpret a file
neo build <file.neo> [-o out.wasm]   compile to a WASI 0.2 component
neo -h | --help                      show help
neo -V | --version                   show the version
```

| Command | Behaviour |
| --- | --- |
| `neo` | Starts an interactive session. Declarations persist until you quit. |
| `neo FILE` | Parses, checks, evaluates `main`, prints its result, and exits with its `U32` status (or `0`). |
| `neo build FILE` | Compiles `main` and everything reachable from it into a component. |
| `neo build FILE -o PATH` | Same, with an explicit output path. `--output PATH` is also accepted. |
| `neo run` / `neo check` | Reserved and rejected with an error. |

Details:

- `neo FILE` requires `fn main(): IO[T]`; interpreting a library-only file is an
  error.
- `neo build` also requires `main`, and `T` must be a printable data type
  (see [section 13](#13-printing-and-building-for-wasi)).
- `neo build` accepts exactly one input file. Unknown `-` options are errors.
- `neo FILE` exits with the low eight bits of a `U32` result from `main`;
  other result types exit `0`.
- `neo build` exits with status 0. Any error is written to stderr and exits
  with status 1.
- Errors in a file include the path, for example:
  `error: 3:7: examples/nat.neo: type mismatch: expected ...`.
- An unknown first argument is treated as a file path, not as a subcommand.
- Every function is re-checked by the kernel after elaboration. A definition
  that passed the surface checks but fails the kernel re-check is rejected with
  a `kernel rejected ...` error.

---

## 3. Lexical structure

Source files are ASCII. The lexer rejects any character it does not recognise.

### Whitespace and comments

Spaces, tabs, and carriage returns are ignored. Line comments start with `//`
and run to the end of the line:

```neo
// This is a comment.
fn main(): IO[Nat] { IO::pure(5) }  // So is this.
```

There are no block comments. There are no semicolons.

### Newlines

Newlines are significant. They separate:

- enum constructors,
- `let` bindings,
- match arms.

A newline **inside parentheses or brackets** is ignored, so argument lists,
parameter lists, and constructor fields may span lines:

```neo
fn add(
  a: Nat,
  b: Nat
): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}
```

Newlines inside `{ ... }` are not ignored.

### Identifiers

An identifier starts with an ASCII letter or `_` and continues with ASCII
letters, digits, `_`, or `'`:

```
x   foo   Nat   Succ'   _tmp   x1
```

The single token `_` is the wildcard, not an identifier.

The style is Rust-like: enum and type names and constructors are CamelCase
(`Shape`, `Vec`, `Circle`, `Vnil`); modules, functions, parameters, and local
bindings are lowercase snake_case (`geometry`, `nat_add`, `index`). This is a
convention, not a parser rule.

### Keywords

```
enum   fn   let   match   Type   Prop
as   impl   mod   pub   use
```

`Refl` is recognised specially in expressions. Everything else — including
`True`, `False`, `Zero`, `Nil`, and `Unit` — is an ordinary name resolved from
the prelude or the program.

### Numeric literals

Numeric literals are unsigned decimal integers:

```neo
0   5   42   1000
```

A numeral has no intrinsic type. It takes its type from the expected type:

- in a `Nat` position it elaborates to a `Nat` built from `Zero` and `Succ`;
- in a `U32` position it elaborates to a machine word;
- in any other position it is rejected.

```neo
fn a(): Nat { 42 }
fn b(): U32 { 42 }
fn c(): Bool { 42 }   // error
```

There is no default. A numeral in a position with no expected type is rejected:

```neo
fn id[T: Type](x: T): T { x }
fn main(): IO[Nat] { IO::pure(id(42)) }          // error
fn main(): IO[U32] { IO::pure(id(42u32)) }       // Ok
fn main(): IO[Nat] { IO::pure(id[Nat](42)) }     // Ok
fn main(): IO[Nat] { IO::pure(id((42 : Nat))) }  // Ok
```

A literal with the `u32` suffix pins the type to `U32`:

```neo
0u32   42u32   4294967295u32
```

The value must fit in 32 bits; `4294967296u32` is an error. The suffix must
not run into a following identifier, so `1u32x` is the numeral `1` followed by
the name `u32x`. A suffixed literal checked against a different type is an
error.

A `Nat` literal above 1,000,000 is rejected because it would build an enormous
unary value. Use a `u32` literal for machine-sized numbers.

There are no negative, floating-point, hexadecimal, or character literals.

### Text and byte literals

A string literal is UTF-8 text in double quotes:

```neo
"hello"
"line one\nline two"
"quote: \" and backslash: \\"
"héllo"
```

Supported escapes are `\n`, `\t`, `\r`, `\\`, `\"`, and `\xNN`. In a string
literal, `\xNN` is limited to `00`–`7f` so the result stays valid UTF-8. String
literals cannot contain an unescaped newline.

A bytes literal is `b"..."`. Its content must be ASCII, and `\xNN` may use any
byte value:

```neo
b"neo"
b"\x00\xff"
```

A string literal inhabits the primitive `String` type and a bytes literal
inhabits the primitive `Bytes` type. Both are immutable byte sequences with
O(1) length and slicing. `String` is expected to hold valid UTF-8; slicing is
byte-based, so slices must start and end on UTF-8 character boundaries to
remain meaningful. `Bytes` is uninterpreted. `String` results print as text and
`Bytes` results print as `b"\xNN\xNN..."` by both the interpreter and
`neo build`.

### Operators and punctuation

```
(  )  [  ]  {  }  ,  .  ::  :  =  ==  ->  =>
+  -  *  /  %  &  |  ^  <<  >>  <  <=  >  >=  !=
```

`[` and `]` delimit implicit parameter and argument lists. `->` is the
function-type arrow; `=>` introduces a lambda body or match arm. `.` selects a
method call. `::` separates the segments of a module path and qualifies a
constructor by its enum.

`=` is both the `let` binding separator and the equality proposition. `==` and
`!=` are boolean equality. `//` starts a line comment; `/` is division when it
is not followed by another `/`. `-` is binary subtraction only; there is no
unary minus.

Operators are non-associative at the comparison level. See
[§7](#numbers-and-operators) for precedence and overloading.

---

## 4. Declarations

A program is a sequence of items:

```neo
mod name { ... }
use path::{Name, other as alias, *}
enum Name { ... }
extern fn capability(parameter, ...): IO[T] = "import"
fn name[implicit, ...](parameter, ...): Result { ... }
impl[implicit, ...] Name[implicit, ...] { ... }
```

Rules:

- `mod`, `use`, `enum`, `extern fn`, `fn`, and `impl` may appear at the top
  level and inside a module. `pub` may prefix `mod`, `use`, `enum`, `extern
  fn`, and `fn`; `impl` blocks are never public, but their methods may be.
- Modules and their items live in one namespace per module. A module may not
  redefine a name already used by an item or by a module at the same path.
- Items are resolved before elaboration and elaborated in dependency order, so
  a module may refer to an item declared later in the same module.
- The only recursion available is a function calling **itself**. Mutual
  recursion is not supported.
- Definitions may be nested inside modules. Function bodies are blocks, which
  may contain `let` bindings and expressions.

For example, `add` may precede `mul` or follow it:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}

fn mul(a: Nat, b: Nat): Nat {
  match a {
    Zero => Zero
    Succ(p) => add(b, mul(p, b))
  }
}
```

### Modules and visibility

`mod name { ... }` groups items. `::` reaches an item through a path:

```neo
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
```

- A `use` path is resolved from the program root unless it starts with `self`
  or `super`: `use geometry::Shape::{Circle, Square}` imports constructors,
  `use geometry::Shape as S` renames a type, and `use geometry::Shape::*`
  imports every constructor. Prefix a binding with `pub` to re-export it to
  the enclosing module's users. A re-export may also re-export a module.
- Inside a module, `self::name` names an item of the same module and
  `super::name` names an item of the enclosing module. A path may start with a
  module, or with a type when reaching one of its constructors.
- Items are private by default and visible in their module and its descendants.
  `pub` makes an item visible everywhere. Methods follow the same rule: a
  method is private unless declared `pub`.
- Types and constructors from the prelude are auto-imported. Other prelude
  items, such as `nat_add`, must be imported with `use nat_add`.
- Module paths use `::`; `.` is only for method calls. `geometry.area(x)` is
  therefore not a module access.

### File modules

`mod name;` loads a module from disk instead of an inline block:

```neo
mod geometry;
```

The loader looks for `geometry.neo` first and then `geometry/mod.neo` in the
declaring module's directory. A module loaded from `geometry.neo` or
`geometry/mod.neo` keeps `geometry/` as the directory for its own children.
Inline modules also create a child directory: `mod outer { mod inner; }`
loads `outer/inner.neo`.

- Ambiguous layouts (both `name.neo` and `name/mod.neo`) and missing files are
  errors.
- A file may only be loaded once in an active chain; cycles are rejected.
- `parse_source` is for in-memory programs. Use `load::parse_file` (or the CLI)
  to expand file modules before elaboration.
- Diagnostic spans currently identify a line and column within the root file;
  errors from a child file do not yet carry that file's path.

An item declared in the current module shadows an auto-imported prelude name.
Constructors are namespaced, so this is valid:

```neo
fn Pair(a: Bool, b: Bool): Bool { ... }
```

The prelude's own `Pair` is `And::Pair`; a local `Pair` hides the simple name
without changing the constructor.

### Host capabilities

`extern` declares a capability provided by the runtime inside a module:

```neo
mod log {
  pub extern fn write(s: String): IO[Result[Unit, IoError]] =
    "wasi:cli/stderr@0.2.9#write"
}
```

The prelude already declares `stderr::write` with this import, so ordinary
programs can call it directly rather than redeclaring it.

Rules:

- The capability is named by its module path: `log::write`. A module is not a
  value, so `log` alone is an error.
- There is no body; instead, `= "import"` names the capability explicitly.
- The result must be `IO[T]`, with no implicit parameters. Parameters must be
  `U32`, `String`, or `Bytes`.
- Import strings come from a closed registry the compiler knows how to lower.
  An unknown import, or a declared type that does not match the registered
  capability type, is an error.
- Externs are not first-class values. Only a fully applied call is valid:
  `stderr::write("x")`, never `let f = stderr::write`.
- `stderr::write` returns `IO[Result[Unit, IoError]]`. The runtime writes the
  string to standard error and reports `Ok(Unit)`, `Err(Closed)`, or
  `Err(Failed)`. `Failed` covers any other host error; the opaque WASI error
  handle is dropped rather than exposed.

A complete program:

```neo
fn exit_code(r: Result[Unit, IoError]): IO[U32] {
  match r {
    Ok(_) => IO::pure(0u32)
    Err(_) => IO::pure(1u32)
  }
}

fn main(): IO[U32] {
  IO::bind(stderr::write("hello\n"), exit_code)
}
```

The interpreter performs the same write and builds the same `Result` value,
so programs agree between `neo file.neo` and the compiled component.

---

## 5. Types and universes

Neo has two kinds of universe:

| Syntax | Meaning |
| --- | --- |
| `Prop` | The sort of propositions. `Prop : Type 0`. |
| `Type` | Shorthand for `Type 0`. |
| `Type 0`, `Type 1`, ... | Predicative type universes. `Type u : Type (u+1)`. |

There are no universe variables or implicit universe levels. Write the level
you mean.

### Function types

```neo
fn(x: Nat) -> Nat
fn(a: Type, x: a) -> a
fn(Nat) -> Nat            // binder name omitted
```

`fn(x: A) -> B` is a dependent function type: `B` may mention `x`.
Multiple binders are nested function types.

### Enum types

```neo
Nat
List[Nat]
Vec[Nat, 2]
```

Parameters of a type constructor are written in `[]`. A function may declare
implicit parameters in `[]` and explicit parameters in `()`; calls omit the
implicit ones by default. See [implicit parameters](#implicit-parameters).

### Primitive types

`U32` is a primitive, opaque 32-bit unsigned integer type:

```neo
U32
```

It has no constructors and cannot be matched. A `U32` is eliminated only by
the primitive operations listed under
[Numbers and operators](#numbers-and-operators). `U32` lives in `Type 0`.

`String` and `Bytes` are primitive, opaque byte sequences:

```neo
String
Bytes
```

Neither has constructors, and neither can be matched. They are used through
the methods listed under [text methods](#text-methods). A value is a descriptor
for a byte buffer with O(1) length and slicing; slicing allocates only a
descriptor, while concatenation allocates a fresh buffer. All three primitive
types live in `Type 0`.

### Action types

`IO[T]` is an opaque type of deferred actions:

```neo
IO[U32]
IO[String]
IO[List[Nat]]
```

`IO[T]` lives in the same universe as `T`. It has no constructors, no matcher,
and no `run`; an action is built with `IO::pure(v)`, `IO::bind(m, k)`, or a
saturated `extern` call. The runtime runs `main`'s action once. `T` must be a
type, not a proposition.

### Equality types

```neo
Zero = Zero
add(2, 3) = 5
```

`=` forms a proposition. Both sides must have the same type. See
[section 9](#9-equality-and-proofs).

### Universe rules

- `Prop` is impredicative: `fn(x: A) -> Prop` is itself a `Prop`, regardless
  of the size of `A`.
- `Type` is predicative: for a non-`Prop` codomain, a function type lives in
  `Type (max u v)` when its domain and codomain live in `Type u` and `Type v`.
- `Type u` is cumulative: a value of `Type u` may be used where `Type v` is
  expected when `u <= v`.
- `Prop` is not a subtype of `Type 0`; `Prop` is a **term** of `Type 0`.

An ascription `(e : T)` checks `e` against `T`. It is the standard way to
direct a numeral or a proof whose type is not determined by the surrounding
context.

---

## 6. Enums

An enum declares an inductive family:

```neo
enum Nat {
  Zero
  Succ(Nat)
}
```

Syntax:

```
enum Name[parameters] : Sort {
  constructor(fields)
  constructor(fields) : ResultType
}
```

- The parameter list is optional and written in `[]`. Parentheses are not
  accepted; `enum E(A: Type)` is an error.
- The sort annotation is optional and must be `Prop` or `Type u`. The default
  is `Type 0`.
- Each constructor is on its own line. A trailing `}` may follow the last
  constructor on the same line.
- A constructor's result type is implicit: `Name` applied to the enum's
  parameters in order.
- An explicit result type is used for indexed families (see below). It must be
  headed by the enum being defined, applied to the declared number of
  parameters; a constructor cannot return a different type.

### Parameters

Parameters may depend on earlier parameters:

```neo
enum Dep[A: Type, x: A] {
  mk(y: A)
}
```

Enum parameters are implicit parameters: they are written in `[]`, and type
application supplies them in `[]`. Constructors take the enum parameters
implicitly, then their fields explicitly. Parameterised constructors therefore
need an expected type; Stage 0 has no explicit constructor type-argument
syntax.

```neo
enum Maybe[T: Type] {
  None
  Some(T)
}
```

this is an error:

```neo
fn bad(): Nat {
  let m = Some(Zero)     // cannot infer the type arguments of `Some`
  0
}
```

The expected type must be known:

```neo
fn five(): Maybe[Nat] {
  Some(5)
}

fn main(): IO[Nat] {
  let m = five()
  IO::pure(0)
}
```

### Fields

Fields are positional. A field may be named:

```neo
enum Pair {
  pair(Nat, Bool)          // two unnamed fields
}

enum Sigma[A: Type, B: fn(x: A) -> Type] {
  sigma(x: A, y: B(x))     // `x` is visible in `B(x)`
}
```

A named field is a **binder for later fields and for the explicit result
type**. It does not make constructor arguments named:

```neo
enum Window {
  window(width: Nat, height: Nat)
}

window(800, 600)    // still positional
```

An unnamed field gets an internal name that is not intended to be referenced
later.

### Indexed families

Named fields and explicit result types let you write indexed types:

```neo
enum Vec[T: Type, n: Nat] {
  Vnil : Vec[T, Zero]
  Vcons(m: Nat, x: T, xs: Vec[T, m]) : Vec[T, Succ(m)]
}

fn two(): Vec[Nat, 2] {
  Vcons(1, 4, Vcons(0, 3, Vnil))
}
```

Here `m` is an ordinary runtime field that records the length of the tail,
and the constructor's result type connects it to `Succ(m)`. Because `Vec` has
parameters, `Vcons(...)` must be used where the expected type is known.

Indexed families support **index refinement**. Matching refines both constructor
fields and ordinary context parameters that the scrutinee's indices determine.
Constructors whose result indices cannot match become impossible branches:

```neo
enum Vec[T: Type, n: Nat] {
  Vnil : Vec[T, Zero]
  Vcons(m: Nat, x: T, xs: Vec[T, m]) : Vec[T, Succ(m)]
}

fn tail[T: Type, n: Nat](v: Vec[T, Succ(n)]): Vec[T, n] {
  match v {
    Vcons(m, x, xs) => xs
  }
}
```

The scrutinee `Vec[Nat, Succ(n)]` cannot be built by `Vnil`, so the `Vnil` arm
may be omitted. In the `Vcons` branch, the equation `Succ(m) = Succ(n)`
refines the field `m` to `n`, so `xs` has type `Vec[Nat, n]`. An arm written for
an impossible constructor is accepted but never checked, and the generated code
omits it.

Context parameters are refined too. The classic length and append functions
work directly on a bare index:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(k) => Succ(add(k, b))
  }
}

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    Vnil => Zero
    Vcons(m, x, xs) => Succ(length(xs))
  }
}

fn append[T: Type, n: Nat, m: Nat](xs: Vec[T, n], ys: Vec[T, m]): Vec[T, add(n, m)] {
  match xs {
    Vnil => ys
    Vcons(k, x, tail) => Vcons(add(k, m), x, append(tail, ys))
  }
}
```

In the `Vnil` branch of `length`, the equation `n = Zero` refines `n`; in the
`Vcons` branch it refines `n` to `Succ(m)`. In `append`, the `Vcons` branch
learns `n = Succ(k)`, so the constructed vector has index
`Succ(add(k, m))`, convertible to `add(Succ(k), m)`.

Indexed families are still limited:

- Refinement is first-order. An index that is stuck, such as `pred(n)` where
  `pred` is an ordinary recursive function, cannot be related to a constructor
  and the match is rejected. A cyclic equation is rejected too.
- A value whose type has a field like `Vec[T, m]` that remains dependent on an
  earlier field cannot be the result of `main`, because it is not printable.

See [`docs/core-spec.md`](core-spec.md) for the exact refinement rule,
`examples/indexed_match.neo` for branch-field refinement, and
`examples/indexed_append.neo` for context refinement.

### Empty enums

An enum may have no constructors:

```neo
enum False : Prop {
}
```

`False` is uninhabited. It is eliminated with a zero-arm match when the result
type is known:

```neo
fn absurd(P: Prop, f: False): P {
  match f {
  }
}
```

The expected type is required because there are no branches from which to
infer a result type.

### No mutual or nested inductives

An enum cannot refer to an enum declared later, and a constructor field may
not contain a nested occurrence of the enum being defined. See
[section 11](#11-positivity).

---

## 7. Functions and expressions

### Function declarations

```
fn name[implicit, ...](parameter, ...): ResultType {
  binding
  ...
  finalExpression
}
```

The bracketed group is optional. Parameters in `[]` are implicit; parameters
in `()` are explicit.

Example:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}
```

- Parentheses are always required for explicit parameters, including when
  there are none: `fn main(): IO[Nat] { ... }`.
- The body is always a block and must end in an expression. There is no
  `return` keyword.
- Parameter types and the result type may depend on earlier parameters:

```neo
fn replicate[T: Type](n: Nat, x: T): List[T] {
  match n {
    Zero => Nil
    Succ(p) => Cons(x, replicate(p, x))
  }
}
```

Here `T` is implicit and inferred from `x`; `n` and `x` are explicit
arguments.

### Blocks and bindings

A block is `{ ... }`. It may start with any number of `let` bindings, each on
its own line, followed by exactly one tail expression:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}

fn main(): IO[Nat] {
  let x: Nat = 2
  let y: Nat = 3
  IO::pure(add(x, y))
}
```

- `let name = expression` introduces a binding.
- `let name: T = expression` adds an annotation.
- Bindings are immutable, sequential, and may shadow earlier names.
- An unannotated binding infers its type. Its value must not be an ambiguous
  numeral: `let x = 42` is rejected; write `let x: Nat = 42` or
  `let x = 42u32`.
- A binding annotation is parsed at comparison precedence, so a function type
  or equality type in an annotation must be parenthesized:
  `let f: (fn(x: Nat) -> Nat) = g` and `let p: (Zero = Zero) = Refl`.
- The value of a block is the value of its final expression.
- A block is itself an expression, so it can appear anywhere an expression is
  allowed.

A parameterised constructor needs a known result type. Annotate the binding:

```neo
fn main(): IO[Nat] {
  let m: List[Nat] = Cons(5, Nil)
  IO::pure(0)
}
```

Alternatively, use a helper function with a known return type:

```neo
fn five(): List[Nat] {
  Cons(5, Nil)
}

fn main(): IO[Nat] {
  let m = five()
  IO::pure(0)
}
```

### Application

Application uses parentheses and commas. Implicit arguments use brackets:

```neo
add(2, 3)
f(x, y, z)
id[Nat](4)          // T supplied explicitly
id[Nat](4)          // T supplied explicitly
```

- Application is left-associative: `f(a)(b)` applies `f` to `a`, then to `b`.
- An empty argument list `f()` is accepted. It may instantiate the function's
  implicit parameters if the context determines them.
- A function's `[]` parameters are implicit; its `()` parameters are explicit.
- `e.f(args)` is a method call: it applies the function named `Type.f`, where
  `Type` is the head of the receiver's type. See [methods](#methods).

### Implicit parameters

A parameter written in `[]` is **implicit**. Calls omit implicit parameters by
default; the elaborator fills them with metavariables and solves them from the
types of the supplied arguments and from the expected result type:

```neo
fn length[T: Type, n: Nat](v: Vec[T, n]): Nat

length(v)                 // T and n inferred from v
append(xs, ys)            // T, n, and m inferred
map(inc, xs)              // T and U inferred from inc and xs
```

To supply implicit arguments explicitly, use brackets:

```neo
length[Nat, 2](v)
append[Nat, 1, 2](xs, ys)
```

A bracket list may supply a prefix of the implicit parameters; any remaining
implicit parameters are still inferred.

Rules:

- Only parameters declared in `[]` can be omitted. A parameter declared in
  `()` must always be supplied; it can never be skipped.
- Every omitted implicit parameter must be determined by the remaining
  arguments or the expected type. Otherwise the program is rejected.
- Only first-order solutions are attempted. An implicit parameter whose value
  would be an applied function is rejected.
- Implicit parameters are elaboration sugar. Core terms still contain every
  argument, so the kernel and code generator are unchanged.

Infix operators and their overload rules are described in
[Numbers and operators](#numbers-and-operators).

### Function types

```neo
fn(x: Nat) -> Nat
fn[T: Type](x: T) -> T
fn(x: Nat, y: Nat) -> Nat
```

A binder group in `[]` is implicit and a group in `()` is explicit. A binder
name may be omitted: `fn(Nat) -> Nat` or `fn[Type](x: x) -> x`.

### Lambdas

```neo
fn(x: Nat) => Succ(x)
fn[T: Type](x: T) => x
fn(a: Nat, b: Nat) => add(a, b)
fn(Nat) => Zero
```

A lambda may bind implicit parameters with `[]`; calls to the resulting
function infer them just like calls to a named declaration.

Lambdas capture their free variables by value. They are first-class values:

```neo
fn apply(f: fn(x: Nat) -> Nat, x: Nat): Nat {
  f(x)
}

fn main(): IO[Nat] {
  IO::pure(apply(fn(y: Nat) => Succ(y), 3))
}
```

Lambdas are for parameters. A zero-argument function is written as a
declaration:

```neo
fn Zero(): Nat {
  0
}
```

(`fn() => e` has no useful meaning and is not the way to write one.)

### Methods

A method is a function declared inside an `impl` block. The first explicit
parameter is the receiver and must have the impl target's type:

```neo
impl String {
  pub fn concat(self: String, other: String): String {
    str_concat(self, other)
  }
}

impl[T: Type] List[T] {
  pub fn length(xs: List[T]): Nat {
    match xs {
      Nil => Zero
      Cons(_, rest) => Succ(rest.length())
    }
  }
}
```

Call a method with `.`:

```neo
"abc".concat("def".slice(0, 1))
xs.length()
xs.length[Nat]()
```

- `impl Type { ... }` adds methods to an enum or a primitive type. The same
  type may have several `impl` blocks, but a method name may only be defined
  once for that type.
- `impl[T: Type] List[T] { ... }` declares the type parameters once and passes
  them to every method as implicit parameters. The impl target must apply the
  impl's own type parameters in order.
- The receiver's type head selects the method, so `String.length`,
  `Bytes.length`, and `List.length` can coexist. Dispatch is static.
- Methods are private unless declared `pub`; a private method is callable only
  from its module and that module's descendants.
- The receiver's type must be known before the method is selected, so the
  receiver is elaborated without an expected type. A bare type variable has no
  methods; annotate it. A bare numeral needs an ascription such as
  `(2 : Nat).inc()`, and a constructor call needs a `let` annotation such as
  `let b: Box[Nat] = box(3)` before `b.get()`.
- A method call must supply every explicit parameter, including an empty `()`.
  Methods are not first-class values: `xs.length` is not an expression.
- Calls chain left-associatively: `xs.concat(ys).length()`.
- Inside a method body, `rest.length()` calls the same method recursively; the
  usual termination checker applies.
- Methods may be declared on `Nat`, `U32`, `Bool`, user enums, and primitive
  types.
- There is no unqualified overloading (`concat(a, b)` is not a method call),
  no type classes or instances, and no user-defined operator overloading.

Method declarations are checked before use: the namespace must be a type, the
first explicit parameter must have that type, and `Type.name` cannot be
declared twice.

### Parentheses

Parentheses group expressions:

```neo
(fn(x: Nat) => Succ(x))(3)
```

### Numbers and operators

`Nat` is the unbounded inductive type used for proofs and indices. `U32` is the
32-bit machine word type used for computation. They are not interchangeable,
and there is no automatic conversion. A numeral takes its type from the
expected type, as described in [§3](#numeric-literals).

The infix operators are:

| Operators | Operand type | Result |
| --- | --- | --- |
| `* / %`, `+ -`, `<< >>`, `&`, `^`, `\|` | `Nat` or `U32` | same as the operands |
| `< <= > >=` | `Nat` or `U32` | `Bool` |
| `== !=` | `Nat`, `U32`, `Bool`, `String`, `Bytes`, or `Unit` | `Bool` |

Precedence, tightest first:

| Level | Operators | Associativity |
| --- | --- | --- |
| 1 | application `f(x)`, `T[...]` | left |
| 2 | `* / %` | left |
| 3 | `+ -` | left |
| 4 | `<< >>` | left |
| 5 | `&` | left |
| 6 | `^` | left |
| 7 | `\|` | left |
| 8 | `< <= > >= == !=` | non-associative |
| 9 | `=` equality type | non-associative |

Arithmetic, bitwise, shift, and comparison operators accept operands of the
same type, either `Nat` or `U32`. The type is chosen from the expected type or
from a non-numeral operand:

```neo
fn a(): Nat { 1 + 2 }          // nat_add
fn b(): U32 { 1 + 2 }          // u32_add
fn c(): U32 { 1u32 + 2 }       // u32_add
fn d(): U32 { 1 + 2u32 }       // u32_add
```

If both operands are bare numerals and there is no expected numeric type, the
operator is rejected:

```neo
fn main(): IO[Bool] { IO::pure(1 < 2) }     // error: cannot infer the numeric type
fn main(): IO[Bool] { IO::pure(1u32 < 2) }  // Ok
```

`==` and `!=` are boolean equality. They are not the equality proposition; that
is `=`. The operand type selects the implementation. Equality is defined for
`Nat`, `U32`, `Bool`, `String`, `Bytes`, and `Unit`; other types need a named
function.

Comparisons are non-associative, so `a < b < c` is an error.

The semantics differ by type:

| Operation | `Nat` | `U32` |
| --- | --- | --- |
| `-` | truncated at zero | wraps modulo `2^32` |
| `/`, `%` | zero divisor yields `0` | zero divisor yields `0` |
| `<<`, `>>` | repeated doubling/halving | shift count uses its low five bits |
| `&`, `\|`, `^` | bit-by-bit on the unary value | machine bitwise |
| `+`, `*` | unbounded | wraps modulo `2^32` |

`Nat` operators are ordinary prelude functions, checked for termination like
any other program. They are suitable for proofs and small indices; use `U32`
for machine-sized computation.

The operators are the surface for every `U32` operation. The underlying
kernel operations are named `u32_add`, `u32_sub`, and so on, but those names
are internal and cannot be written in a program.

```neo
fn is_even(n: U32): Bool {
  n % 2u32 == 0u32
}
```

Semantics follow the WebAssembly `i32` operations:

- `+`, `-`, and `*` wrap modulo `2^32`;
- `/` and `%` are total: a zero divisor yields `0`;
- `<<` and `>>` use the low five bits of the shift count.

An operation reduces when both operands are literals, so a `match` on a
comparison reduces during elaboration:

```neo
fn pick(): Nat {
  match 1u32 < 2u32 {
    True => 10
    False => 20
  }
}
```

`U32` has no constructors, so `match` cannot eliminate it directly; compare it
to obtain a `Bool`. A `U32` may still be an enum parameter or an index, as in
`Vec[T, n]` with `n: U32`.

`U32` and `Nat` are reserved, the internal `u32_*` names are reserved, and the
`nat_*` prelude operators are already defined: a program cannot redeclare any
of them.

### Text methods

`String` and `Bytes` expose methods for length, slicing, concatenation, and
conversion. The prelude declares them in terms of the low-level primitive
operations:

| Method | Type | Meaning |
| --- | --- | --- |
| `s.length()` | `fn(String) -> U32` | byte length |
| `s.slice(start, end)` | `fn(String, U32, U32) -> String` | O(1) view from `start` to `end` |
| `s.concat(t)` | `fn(String, String) -> String` | allocate and copy both |
| `s.to_bytes()` | `fn(String) -> Bytes` | view the same bytes as `Bytes` |
| `b.length()` | `fn(Bytes) -> U32` | byte length |
| `b.slice(start, end)` | `fn(Bytes, U32, U32) -> Bytes` | O(1) view from `start` to `end` |
| `b.concat(t)` | `fn(Bytes, Bytes) -> Bytes` | allocate and copy both |
| `b.get(index)` | `fn(Bytes, U32) -> U32` | byte at an index, `0` out of bounds |
| `b.to_str()` | `fn(Bytes) -> Result[String, Utf8Error]` | validate UTF-8 and view the bytes as a `String` |

```neo
fn shout(s: String): String {
  s.slice(0, 1).concat("!")
}
```

Slicing is total: a start or end past the length is clamped to the length, and
`end < start` yields the empty slice. `slice` does not copy; `concat` allocates
a fresh buffer. A `String` is expected to contain UTF-8, so string slices
should start and end on character boundaries; a `String` is not validated when
it is created or sliced.

`b.to_str()` is the checked conversion from `Bytes` to `String`. It validates
strict UTF-8 (rejecting overlong forms, surrogates, code points above
`U+10FFFF`, and truncated sequences) and returns
`Result[String, Utf8Error]`:

```neo
fn parse(bytes: Bytes): Result[String, Utf8Error] {
  bytes.to_str()
}
```

An error carries the byte index where the first invalid sequence starts:

```neo
match b"a\xff".to_str() {
  Ok(text) => text
  Err(Invalid(index)) => ...
}
```

The index is `0`-based, so the example reports `1`. A truncated sequence
reports its first byte.

`==` and `!=` compare text with the internal `str_eq`/`str_ne` and
`bytes_eq`/`bytes_ne` operations. The `str_*` and `bytes_*` operations remain
available as low-level builtins for the prelude; ordinary programs should use
the methods. The prelude declares no `U32` methods because every built-in
operation has an operator; users may still declare methods on `U32`.

---

## 8. Pattern matching

`match` is the only eliminator:

```neo
match scrutinee {
  pattern => expression
  pattern => expression
}
```

- The scrutinee must be an enum value or an equality proof.
- Each arm is on its own line. The closing `}` may follow the last arm on the
  same line.
- Every **reachable** constructor of the enum must be covered (exhaustiveness).
  Constructors whose result indices cannot match the scrutinee type are
  impossible: their arms may be omitted, and arms written for them are ignored.
  See [indexed families](#indexed-families).
- Several arms may start with the same constructor. They are tried in source
  order; the first matching row wins. The nested patterns within all arms for
  one constructor must cover every reachable value they inspect.
- An enum with no constructors, or a scrutinee type whose constructors are all
  impossible, is matched with zero arms; an expected result type is required.

Example:

```neo
fn pred(n: Nat): Nat {
  match n {
    Zero => Zero
    Succ(p) => p
  }
}
```

### Patterns

```
_                        wildcard
x                        variable binder
Zero                     nullary constructor
Succ(p)                  constructor with sub-patterns
Cons(_, t)               wildcard field
Cons(Succ(k), rest)      nested constructor pattern
```

Patterns nest arbitrarily. A nested pattern compiles to an inner `match` on the
field, using the same index refinement as an ordinary match:

```neo
fn classify(n: Nat): Nat {
  match n {
    Zero => Zero
    Succ(Zero) => 1
    Succ(Succ(Zero)) => 2
    Succ(Succ(Succ(k))) => 3
  }
}
```

Multiple arms for one constructor are allowed and checked in order:

```neo
fn small(xs: List[Nat]): Nat {
  match xs {
    Nil => Zero
    Cons(Zero, rest) => Zero
    Cons(Succ(k), rest) => k
  }
}
```

Each nested match must cover the values it inspects. If an index refinement
makes a nested value a known constructor, patterns for the other constructors
are simply impossible and may be omitted.

Restrictions:

- There are no pattern guards, or-patterns, as-patterns, or literal patterns.
- A single arm whose pattern is a variable or `_` is an irrefutable binding:

```neo
fn force(n: Nat): Nat {
  match n {
    x => x
  }
}
```

- If that single variable's name is a nullary constructor, it is treated as
  the constructor, not as a binder. For example, a lone `Zero => ...` arm is a
  constructor pattern for `Nat`, and the match is then rejected as
  non-exhaustive. Use `_` or a different name for an irrefutable arm.

### Dependent results

If the expected result type is known and the scrutinee is a variable, the
result type is abstracted over that variable, so it may mention it. Indices are
refined from the scrutinee type (see
[indexed families](#indexed-families)): solving `Succ(m) = Succ(n)` gives the
branch field `m` the value `n`, and field types that mention `m` are rewritten
accordingly. Solving `Succ(m) = n` refines the context parameter `n` to
`Succ(m)`, so later branches and recursive calls see the refined type.

Refinement is first-order and total. It does not guess: a stuck index such as
`pred(n)`, or an equation that would make an index depend on itself, is
rejected instead of being assumed.

### Matching on equality proofs

A match on a proof of `x = y` is special. It takes exactly one arm, whose
pattern is `Refl` or `_`:

```neo
fn sym[T: Type](x: T, y: T, p: x = y): y = x {
  match p {
    Refl => Refl
  }
}
```

The arm is checked against a motive. If the expected type is written in terms
of the right-hand side `y`, that occurrence is abstracted to the motive
argument; the `Refl` arm is then checked at the left-hand side `x`, and the
whole match has the expected type with `y` in place. This is motive-based
equality elimination (`J`), not textual replacement.
See [section 9](#9-equality-and-proofs).

Abstraction is capture-avoiding, so it works under binders. For example, an
expected type that mentions `x` inside a function type is abstracted too:

```neo
fn h(x: Nat, p: x = Zero): fn(y: Nat) -> Zero = x {
  match p {
    Refl => fn(y: Nat) => Refl
  }
}
```

A `match` whose scrutinee is a compound expression is abstracted the same
way: the motive replaces every occurrence of that expression. Abstraction is
syntactic, so an occurrence that is merely convertible to the scrutinee is
not found; that case needs an explicit helper.

---

## 9. Equality and proofs

`=` is a built-in homogeneous equality proposition:

```neo
Zero = Zero
add(2, 3) = 5
```

The type of the left side determines the type of the right side; both must be
the same type.

### `Refl`

`Refl` proves `x = y` when `x` and `y` are definitionally equal. It is only
valid where the expected type is a known equality:

```neo
fn refl_zero(): Zero = Zero {
  Refl
}
```

Writing `Refl` in an inference position is an error:

```
`Refl` needs an expected equality type; use it where the type is known
```

If the sides are not convertible:

```
`Refl` cannot prove `Zero = Succ(Zero)` (sides are not equal)
```

### Proving with recursive matches

Proofs are ordinary total functions. Recursive `match` replaces tactics such
as `induction` (using the `sym` defined above):

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}

fn add_zero(a: Nat): add(a, Zero) = a {
  match a {
    Zero => Refl
    Succ(p) => match sym(add(p, Zero), p, add_zero(p)) {
      Refl => Refl
    }
  }
}
```

There are no tactics, no `theorem` keyword, and no `forall`. A theorem is a
`fn` returning a proposition.

### What can be eliminated

- Equality proofs (`match p { Refl => ... }`) may produce any type, including
  computational data. The proof is erased at run time.
- A proposition enum (`And`, `Or`, `Exists`, `True`) is intended for
  proof-level computation only. The kernel enforces this: eliminating such a
  value into a computational type is rejected (large elimination).
- The exception is the *singleton* rule. A `Prop` enum with no constructors
  (`False`), or with exactly one constructor whose fields are all propositions
  (for example `x = y` with its `Refl` constructor), may be eliminated into any
  type. This is what makes equality proofs usable as ordinary values.
- `False`, which has no constructors, is eliminated with a zero-arm match.
  The expected result type must be known.

### Proof erasure

At run time, types and type-level terms carry no information. Equality proofs
and `J` compile to their `Refl` case. A proof can be bound and ignored:

```neo
fn main(): IO[Nat] {
  let proof = add_zero(3)
  IO::pure(7)
}
```

Proposition-enum values such as `Trivial` are ordinary runtime values at
Stage 0; this is why `fn main(): IO[True] { IO::pure(Trivial) }` can print `Trivial`.

### Proof irrelevance

`Prop` is impredicative and proof terms erase at run time. The current
conversion checker does not yet treat arbitrary proofs of the same
proposition as interchangeable; write proofs in the form the checker expects.

---

## 10. Totality and termination

Every `fn` must terminate. The definitive check runs on the elaborated core
term inside the kernel, so surface sugar (parentheses, blocks, variable
patterns, aliases) cannot hide a recursive occurrence. It requires:

1. **The recursive name never escapes.** It may only be the head of a fully
   applied call (`f(x, y)`); every other occurrence (`g = f`, `f` passed to
   another function, `f` returned, captured, or stored) is rejected even when
   the decrease condition would hold.
2. **Every call is structurally decreasing in one fixed argument position**
   shared by all recursive calls.
3. The decreasing argument must be a **variable** bound by a pattern
   destructure of a parameter (or of an already-smaller variable), at any
   nesting depth. Nested constructor patterns expose inner fields as smaller
   values.

Accepted:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}
```

Here every call decreases in argument 0 (`p` is a subterm of `a`).

Rejected:

```neo
fn bad(n: Nat): Nat {
  bad(n)                    // does not decrease
}

fn bad2(n: Nat): Nat {
  bad2(Succ(n))             // increases
}

fn bad3(n: Nat): Nat {
  match n {
    Zero => Zero
    Succ(p) => bad3(Succ(p))  // non-variable expression containing a smaller value
  }
}
```

Exact error styles:

```
recursive call to `bad` is not structurally decreasing; no argument is a strict subterm of a parameter
recursive calls to `loop` do not decrease in a single consistent argument position
recursive function `boom` is used as a value; recursive calls must be fully applied
```

The “used as a value” error is what rejects indirect recursion such as:

```neo
fn apply(g: fn(Nat) -> False, x: Nat): False { g(x) }
fn boom(n: Nat): False { apply(boom, n) }     // rejected
```

Consequences:

- Only self-recursion is possible.
- A function can never be passed, returned, stored, or captured while it is
  being defined, which keeps the termination argument purely syntactic.
- Recursion on a computed value is rejected even if it is obviously total.
- The checker is deliberately conservative; some total functions need to be
  rewritten in a structurally recursive style.
- A `match` whose scrutinee is not a variable does not make its fields
  smaller, so recursion on those fields is rejected.

The checker is part of the trusted kernel: after elaboration, the whole
program is re-checked from the stored core terms, and only functions that pass
both the recursion check and the type checker have unfoldable bodies. A
definition that fails re-checking is rejected even if it was elaborated
successfully.

---

## 11. Positivity

Every enum must be strictly positive. For each constructor field type, the
enum being defined:

- may occur **directly**: `T`, `List[T]`, `Vec[T, n]`;
- may not occur to the left of an arrow: `fn(x: T) -> Nat`;
- may not occur as an argument of another type constructor:
  `List[T]` inside `T`'s own definition is a nested inductive and is rejected;
- may not be hidden inside an application or match.

Rejected:

```neo
enum Bad {
  bad(List[Bad])            // nested inductive
}

enum Negative {
  negative(fn(x: Negative) -> Nat)   // negative occurrence
}
```

Error:

```
`Bad` is not strictly positive in this constructor field (nested inductives are not supported)
```

The same applies to constructor result indices. The enum being defined may not
occur anywhere inside them, so this is rejected as well:

```neo
enum Bad[F: Type 1] : Type 1 {
  mk(g: F) : Bad[fn(x: Bad[F]) -> Type 0]
}
```

```
`Bad` occurs in a constructor result index; enum indices may not mention the enum being defined
```

```neo
enum E[A: Type 1] : Type 0 {
  c(x: A)                  // rejected: a `Type 1` field in a `Type 0` enum
}
```

```
constructor field lives in `Type 1`, but `E` is declared in `Type 0`; a field may not be larger than its enum
```

### Universe bounds on inductives

For an enum declared at `Type u`, every constructor field type and index type
must live in a sort at most `Type u`. Fields may not be larger than the enum:

```neo
enum E[A: Type 1] : Type 0 {
  c(x: A)                  // rejected: a `Type 1` field in a `Type 0` enum
}
```

A `Prop` enum may have large fields; it is the elimination, not the
declaration, that is restricted (see the singleton rule above).

There are no mutual inductive types, because every enum must be declared
before it is used.

---

## 12. The prelude

Every program and REPL session starts with these declarations already in
scope. They are shown for reference; do not redeclare them in a program.

```neo
enum Nat {
  Zero
  Succ(Nat)
}

enum Bool {
  False
  True
}

enum Unit {
  Unit
}

enum List[T: Type] {
  Nil
  Cons(T, List[T])
}

enum False : Prop {
}

enum True : Prop {
  Trivial
}

enum And[P: Prop, Q: Prop] : Prop {
  Pair(P, Q)
}

enum Or[P: Prop, Q: Prop] : Prop {
  Left(P)
  Right(Q)
}

enum Exists[T: Type, P: fn(x: T) -> Prop] : Prop {
  Witness(x: T, P(x))
}

enum Option[T: Type] {
  None
  Some(T)
}

enum Result[T: Type, E: Type] {
  Ok(T)
  Err(E)
}

enum Utf8Error {
  Invalid(U32)
}

enum IoError {
  Closed
  Failed
}
```

The prelude also defines these numeric, boolean, and unit functions. They are
not auto-imported; write `use nat_add` (and so on) to bring one into scope:

```neo
fn bool_not(b: Bool): Bool
fn bool_eq(a: Bool, b: Bool): Bool
fn bool_ne(a: Bool, b: Bool): Bool

fn nat_add(a: Nat, b: Nat): Nat
fn nat_sub(a: Nat, b: Nat): Nat
fn nat_mul(a: Nat, b: Nat): Nat
fn nat_div(a: Nat, b: Nat): Nat
fn nat_rem(a: Nat, b: Nat): Nat
fn nat_eq(a: Nat, b: Nat): Bool
fn nat_ne(a: Nat, b: Nat): Bool
fn nat_lt(a: Nat, b: Nat): Bool
fn nat_le(a: Nat, b: Nat): Bool
fn nat_gt(a: Nat, b: Nat): Bool
fn nat_ge(a: Nat, b: Nat): Bool
fn nat_and(a: Nat, b: Nat): Nat
fn nat_or(a: Nat, b: Nat): Nat
fn nat_xor(a: Nat, b: Nat): Nat
fn nat_shl(a: Nat, b: Nat): Nat
fn nat_shr(a: Nat, b: Nat): Nat
fn nat_div2(a: Nat): Nat
fn nat_mod2(a: Nat): Nat

fn unit_eq(a: Unit, b: Unit): Bool
fn unit_ne(a: Unit, b: Unit): Bool
```

Text methods are declared with `impl` blocks:

```neo
impl String {
  pub fn length(self: String): U32
  pub fn slice(self: String, start: U32, end: U32): String
  pub fn concat(self: String, other: String): String
  pub fn to_bytes(self: String): Bytes
}

impl Bytes {
  pub fn length(self: Bytes): U32
  pub fn slice(self: Bytes, start: U32, end: U32): Bytes
  pub fn concat(self: Bytes, other: Bytes): Bytes
  pub fn get(self: Bytes, index: U32): U32
  pub fn to_str(self: Bytes): Result[String, Utf8Error]
}
```

The prelude also declares host-capability modules:

```neo
mod stderr {
  pub extern fn write(s: String): IO[Result[Unit, IoError]]
}

mod stdout {
  pub extern fn write(s: String): IO[Result[Unit, IoError]]
}

mod env {
  pub extern fn argv(): IO[List[String]]
  pub extern fn args(): IO[List[String]]
  pub extern fn get(name: String): IO[Option[String]]
}

mod stdin {
  pub extern fn read_all(): IO[Result[Bytes, IoError]]
}

mod file {
  pub extern fn read(path: String): IO[Result[Bytes, IoError]]
  pub extern fn write(path: String, data: Bytes): IO[Result[Unit, IoError]]
}

mod dir {
  pub extern fn create(path: String): IO[Result[Unit, IoError]]
  pub extern fn list(path: String): IO[Result[List[String], IoError]]
}
```

`file` paths are resolved relative to a preopened directory. Under `wasmtime`,
grant one with `--dir .`; the interpreter uses the process working directory.

Notes:

- A numeral in a `Nat` position desugars to `Zero` and `Succ`; in a `U32`
  position it is a primitive literal. The `u32` suffix pins `U32`.
- `String` and `Bytes` literals and operations are primitive; their public
  surface is the method set in [text methods](#text-methods).
- `List` is `List[T]`, not a built-in vector; it is a linked list.
- `Option` and `Result` are ordinary enums. `main`'s action may produce either;
  both backends print constructors as `Some(3)`, `Ok("x")`, or
  `Err(Invalid(2))`.
- `And`, `Or`, `Exists`, `True`, and `False` are propositions (`Prop`).
- Types and constructors are auto-imported by their simple names. Because
  `Bool`'s `True` and the proposition `True` share a spelling, a bare `True`
  in a position where the expected type is not known is ambiguous; write
  `Bool::True` or qualify it.
- `U32`, `String`, `Bytes`, the `u32_*` operations, and the low-level
  `str_*`/`bytes_*` operations are primitive. The `u32_*` names are internal
  and report the corresponding operator; `str_*` and `bytes_*` cannot be
  written in a program at all.
- `Nat` arithmetic is provided by the `nat_*` prelude functions and the infix
  operators; `U32` uses the same operators over primitive machine words.

---

## 13. Printing and building for WASI

### Requirements for `main`

`main` must be a `fn` declaration with **no parameters** and type `IO[T]`:

```neo
fn main(): IO[U32] {
  IO::pure(0u32)
}
```

`T` must be printable.

A type is printable when it is:

- `Nat`, `Bool`, `Unit`, `String`, `Bytes`, or `U32`;
- an enum whose type arguments are printable and whose constructor field types
  are all printable.

This includes ordinary parameterised enums such as `List[Nat]` and a
user-defined `Maybe[Nat]`, and proposition enums with printable fields such as
`True`. The following are **not** printable:

- function types;
- `Type` and `Prop`;
- equality types such as `x = y`;
- enums with a field whose type is a type family applied to an earlier field,
  such as `Vec[T, m]` in the indexed `Vec` example.

A program whose `main` result is not printable is rejected by both the
interpreter and `neo build`:

```
the result type of `main` is not printable: `Vec[Nat, 2]`
```

A `main` whose type is not `IO[T]` is rejected with
`` `main` must have type `IO[T]` ``.

### Printed form

Values print with surface constructor names and without type parameters:

```neo
Cons(2, Cons(1, Cons(0, Nil)))     // List[Nat]
Cons(5, Nil)                       // List[Nat]
pair(4, True)
Unit
True
42                                 // Nat numerals are printed in decimal
42                                 // U32 values print the same, without the suffix
Trivial                            // True
"hello"                            // String prints as raw text
b"\x00\xff"                        // Bytes print as escaped bytes
```

Index fields declared as normal fields are printed too. The `Vec` example
prints `Vcons(1, 4, Vcons(0, 3, Vnil))`, where `1` is the recorded tail
length.

The runtime prints one newline after the value.

### Generated component

`neo build` produces a **WASI 0.2 component**, not a core module:

- export: `wasi:cli/run@0.2.9#run`;
- imports: `wasi:cli/stdout@0.2.9` and `wasi:cli/stderr@0.2.9` for printing
  and host writes, plus `wasi:io/streams@0.2.9` and `wasi:io/error@0.2.9`;
- memory is exported;
- the runtime runs `main`'s action once, prints the produced value, and returns
  exit code 0.

Run it with any WASI 0.2 runtime:

```sh
wasmtime program.wasm
```

Only functions reachable from `main` are compiled. Unused proofs and type-level
functions do not affect code generation. Types and type-level terms are erased;
equality proofs and `J` reduce to their `Refl` case. Proposition-enum values
remain ordinary values and can be printed. The runtime allocates with a bump
allocator; memory grows on demand. There is no garbage collector and no
reference counting.

The I/O API is `IO::pure`, `IO::bind`, and the `extern` capabilities in the
closed registry. The prelude provides `stdout::write`, `stderr::write`,
`env::argv`, `env::args`, `env::get`, `stdin::read_all`, `file::read`,
`file::write`, `dir::create`, and `dir::list`. File and directory paths are
relative to a preopened directory; pass `--dir .` to `wasmtime` to grant the
current one. `env::args` excludes the program name; `env::argv` includes it.

When `main`'s action produces `U32`, that value is printed and its low eight
bits become the process exit status. Other result types print normally and
exit `0`. The interpreter and compiled component agree on the status.

---

## 14. The REPL

Run `neo` with no arguments:

```
neo 0.1.0 (type :help for help)
neo>
```

The REPL accepts two kinds of input:

1. **Declarations** starting with `mod `, `use `, `enum `, `extern `, `impl `,
   or `fn <name>(`, optionally prefixed by `pub `. They are added to the current
   environment:
   ```neo
   neo> fn inc(x: Nat): Nat { Succ(x) }
   Ok
   neo> inc(4)
   5
   ```

2. **Expressions**, which are inferred, evaluated, and printed:
   ```neo
   neo> inc(3)
   4
   neo> Type
   Type 0
   neo> Prop
   Prop
   ```

REPL commands:

```
:help    show help
:quit    leave the REPL (also :q and :exit)
```

Multiline input:

- If the line contains unbalanced `{`, the REPL prompts with `...>` until the
  braces balance. This lets you enter multi-line functions and enums.
- Parentheses alone do not trigger continuation.

Display limits: the REPL prints data values (constructors, `Nat`, `Type`,
`Prop`). It cannot display:

- function values or function types;
- equality proofs (for example, a proof of `2 = 2` cannot be displayed);
- bare type constructors other than `Nat`, e.g. `List[Nat]`.

REPL errors are printed and the session continues. The REPL does not require a
`main` function.

---

## 15. Grammar

A simplified EBNF. `NEWLINE` is significant outside parentheses.

```ebnf
program      ::= { NEWLINE } { item { NEWLINE } }

item         ::= [ "pub" ] ( mod_decl | use_decl | enum_decl
                           | extern_decl | fn_decl )
               | impl_decl
mod_decl     ::= "mod" IDENT ( ";" | "{" { NEWLINE } { item NEWLINE } "}" )
use_decl     ::= "use" use_tree
use_tree     ::= path [ "as" IDENT ]
               | path "::" "*"
               | path "::" "{" use_tree { "," use_tree } "}"
path         ::= IDENT { "::" IDENT }

enum_decl    ::= "enum" IDENT [ "[" params "]" ]
                 [ ":" expr ]
                 "{" { NEWLINE } { ctor NEWLINE } "}"
ctor         ::= IDENT [ "(" fields ")" ] [ ":" expr ]

extern_decl  ::= "extern" "fn" IDENT [ "[" params "]" ] "(" params ")"
                 ":" cmp_expr "=" STRING

fn_decl      ::= "fn" IDENT [ "[" params "]" ] "(" params ")" ":" expr block
impl_decl    ::= "impl" [ "[" params "]" ] app
                 "{" { NEWLINE } { [ "pub" ] fn_decl NEWLINE } "}"

params       ::= [ param { "," param } ]
param        ::= IDENT ":" expr

fields       ::= [ field { "," field } ]
field        ::= [ IDENT ":" ] expr

block        ::= "{" { NEWLINE } { binding NEWLINE } expr { NEWLINE } "}"
binding      ::= "let" IDENT [ ":" ann ] "=" expr
ann          ::= cmp_expr

expr         ::= fn_expr | eq_expr

eq_expr      ::= cmp_expr [ "=" cmp_expr ]

cmp_expr     ::= bitor_expr [ cmp_op bitor_expr ]
cmp_op       ::= "==" | "!=" | "<" | "<=" | ">" | ">="

bitor_expr   ::= bitxor_expr { "|" bitxor_expr }
bitxor_expr  ::= bitand_expr { "^" bitand_expr }
bitand_expr  ::= shift_expr { "&" shift_expr }
shift_expr   ::= add_expr { ( "<<" | ">>" ) add_expr }
add_expr     ::= mul_expr { ( "+" | "-" ) mul_expr }
mul_expr     ::= app { ( "*" | "/" | "%" ) app }

app          ::= atom { postfix }
postfix      ::= "[" args "]"
               | "(" args ")"
               | "." IDENT [ "[" args "]" ] "(" args ")"
args         ::= [ expr { "," expr } ]

atom         ::= IDENT { "::" IDENT }
               | NUMERAL
               | STRING
               | BYTES
               | "Type" [ NUMERAL ]
               | "Prop"
               | "(" expr ")"
               | "(" expr ":" expr ")"
               | block
               | match_expr
               | fn_expr

match_expr   ::= "match" expr "{" { NEWLINE } { arm NEWLINE } "}"
arm          ::= pattern "=>" expr

pattern      ::= "_"
               | IDENT { "::" IDENT } [ "(" [ pattern { "," pattern } ] ")" ]
```

Notes:

- `IDENT` excludes the keywords and `_`.
- `NUMERAL` is a decimal numeral with an optional `u32` suffix. Its type comes
  from the expected type; the suffix pins `U32`.
- `STRING` is a double-quoted UTF-8 literal (type `String`) and `BYTES` is a
  `b"..."` byte literal (type `Bytes`).
- A final constructor or match arm may be followed by `}` instead of a
  newline.
- A bare `Refl` name is parsed as the `Refl` expression.
- A lone identifier in a pattern is a binder unless it names a nullary
  constructor of the scrutinee's enum.
- `[` and `]` delimit implicit parameter and argument lists. An empty `[]`
  list is rejected.
- Newlines inside `[...]` are ignored, just like newlines inside `(...)`.
- A method is declared inside `impl Type { ... }`; its first explicit
  parameter must have type `Type`. `impl[T: Type] Type[T] { ... }` declares
  the parameters for all of its methods.
- A postfix `.name(args)` is a method call. It requires the argument list,
  even when the method takes none: `s.length()`. It cannot be partially
  applied or used as a value.
- `IO[T]`, `IO::pure(e)`, and `IO::bind(m, k)` are built-in forms. `IO` takes one
  type in `[]`; the two operations take their arguments in `()`. Their names
  are reserved and cannot be redeclared at the top level.

---

## 16. Errors

Errors are reported as:

```
line:column: message
```

and file errors are prefixed with the path by the CLI.

Common messages:

| Message | Meaning |
| --- | --- |
| `expected an enum or fn declaration` | Top-level input is not a declaration. |
| `a block must end in an expression` | A block ended with a binding or was empty. |
| `expected a newline after a binding` | Bindings must be one per line. |
| `unterminated string literal` | Add the closing `"`. |
| `string literals cannot contain a newline` | Write `\n` instead. |
| `unknown string escape ...` | Use `\n`, `\t`, `\r`, `\\`, `\"`, or `\xNN`. |
| `expected two hex digits after \x` | Write exactly two hexadecimal digits. |
| `\x above 7f is not valid UTF-8; use a bytes literal` | A `String` must stay valid UTF-8. |
| `bytes literals must contain only ASCII characters; use \x escapes` | Non-ASCII bytes need `\xNN` in `b"..."`. |
| `unknown name ...` | Name is undeclared or declared later. |
| `cannot infer the type of this numeral; ...` | A bare numeral has no expected type. Add an annotation, an ascription, or the `u32` suffix. |
| `literal suffix u32 conflicts with expected type ...` | A `u32` literal was checked against a non-`U32` type. |
| `literal does not fit in U32` | The value is above `2^32 - 1`. |
| `Nat literal is too large ...` | `Nat` literals are limited to 1,000,000; use `u32`. |
| `numeric literal cannot have type ...` | A numeral was checked against a non-numeric type. |
| `cannot infer the numeric type of ...; annotate an operand` | Both operator operands are bare numerals and there is no expected type. |
| `boolean equality is not defined for type ...` | `==`/`!=` only cover `Nat`, `U32`, `Bool`, `String`, `Bytes`, and `Unit`. |
| `comparison operators are not associative; parenthesize` | Write `(a < b) == c` or split the expression. |
| `equality is not associative; parenthesize` | Write `(a = b) = c` only if that is intended. |
| `parenthesize a function-type annotation` | Write `let f: (fn(x: T) -> U) = g`. |
| `type mismatch: expected ..., found ...` | Expression has the wrong type. |
| `no method ... on type ...` | The receiver's type has no method with that name. |
| `cannot select a method on type ...` | The receiver's type is not concrete; annotate it. |
| `method ... takes N arguments after the receiver, got M` | Supply every explicit argument, including `()`. |
| `the first parameter of ... must have type ...` | A method's receiver must be the first explicit parameter and have the namespace type. |
| `... is not an enum or primitive type` | An `impl` target must be an enum or a primitive type. |
| `... is private here` | The item or method is private to another module; mark it `pub`. |
| `module ... is private here` | The module is private to another module; mark it `pub`. |
| `a method call needs (...) arguments` | Write `x.f()`, not `x.f`. |
| `... is internal; use the ... operator` | The `u32_*` names are not surface syntax; use the operator. |
| `primitive ... takes N arguments, got M` | A primitive operation must be fully applied. |
| `match: the scrutinee is not an enum value ...` | `String`, `Bytes`, and `U32` cannot be matched; use their operations. |
| `cannot infer the type arguments of ...; use it where the expected type is known` | Parameterised constructor in an inference position. |
| `cannot infer implicit argument of type ...; pass it explicitly with [...]` | An implicit argument is not determined by the rest of the call. |
| `an implicit parameter list cannot be empty` / `an implicit argument list cannot be empty` | Remove the `[]` or add an entry. |
| `enum parameters must be written in [], not ()` | Enum parameters are implicit and use `[]`. |
| `the parameters of ... are written in [], not ()` | A type constructor was applied with `()`. |
| `constructor type arguments are inferred from the expected type; remove the [...] arguments` | Constructors do not accept explicit bracket arguments in Stage 0. |
| `match is not exhaustive: missing constructor ...` | Add the missing arm. |
| `cannot infer the result type of a match on an empty enum; add an expected type` | Empty enums are eliminated with zero arms, but need a known result type. |
| `match: cannot eliminate this proposition into a type` | Large elimination of a `Prop` enum was attempted. |
| `... is larger than its enum` / `... field lives in ...` | A `Type u` enum has a constructor field or index in a larger sort. |
| `... occurs in a constructor result index ...` | The enum appears in a constructor result index. |
| `recursive function ... is used as a value` | The recursive name escaped; only fully applied calls are allowed. |
| `recursive call to ... is not structurally decreasing ...` | A recursive call does not descend on a subterm of one fixed parameter. |
| `constructor ... must return ...` | A constructor's explicit result type must be its declaring enum. |
| `a match on an equality proof must use the pattern Refl` | Equality matches have one `Refl`/`_` arm. |
| `recursive call ... is not structurally decreasing ...` | Termination check failed. |
| `... is not strictly positive ...` | Positivity check failed. |
| `this program needs fn main(): IO[T]` | The file has no `main`. |
| `main must have type IO[T]` | `main` was declared with parameters or another result type. |
| `the result type of main is not printable: ...` | Pick a printable result type for the action. |

---

## 17. Limitations and not-yet-supported

The following are **not** part of Stage 0, even if they appear in the long-term
design:

- references and ownership modes: `&T`, `&mut T`, `own T`;
- arrays and growable vectors: the planned `Array[T, n]` and `Vec[T]`. (The
  indexed `Vec` in [section 6](#6-enums) is a user-defined demonstration, not
  the planned growable vector.)
- traits or type classes. Implicit parameters are declared with `[]` and
  inferred at calls; see [implicit parameters](#implicit-parameters);
- unqualified overloading (`concat(a, b)`), method values (`xs.length`), and
  user-defined operator overloading. Methods are selected only by the
  receiver's type head; see [methods](#methods);
- package managers, module search paths, and path attributes for file
  modules; see [file modules](#file-modules);
- an explicit recursion budget for extremely deep terms; the current compiler
  uses larger stacks instead;
- characters, floats, networking, and I/O beyond argument, stdin, and regular-file access;
- character-aware string slicing: `slice` indexes bytes;
- mutual recursion and mutual inductive types;
- nested or higher-order inductive occurrences;
- higher-order or non-first-order dependent pattern matching: indices are
  refined only when they unify structurally with a constructor result. Context
  and field refinement are supported (see [section 6](#6-enums)); a stuck index
  such as `pred(n)` is rejected;
- proof irrelevance in the conversion checker;
- custom printing and user-defined entry points;
- `def`, `<-`, mutable variables, `proc`, `theorem`, `forall`, `lambda`,
  `case`, and `induction`. Use `fn`, `let` bindings, `fn(...) => ...`, and
  `match` instead.

`main` result types with a field like `Vec[T, m]` that stays dependent on an
earlier field are also not buildable, because they are not printable.

---

## 18. Worked examples

All examples below are in the [`examples/`](../examples) directory and are
checked by the test suite.

### Addition and multiplication

`examples/nat.neo`:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}

fn mul(a: Nat, b: Nat): Nat {
  match a {
    Zero => Zero
    Succ(p) => add(b, mul(p, b))
  }
}

fn main(): IO[Nat] {
  IO::pure(mul(6, 7))
}
```

Output: `42`.

### Booleans and `match`

`examples/bool.neo`:

```neo
fn not(b: Bool): Bool {
  match b {
    True => False
    False => True
  }
}

fn both(a: Bool, b: Bool): Bool {
  match a {
    True => b
    False => False
  }
}

fn main(): IO[Bool] {
  IO::pure(both(not(False), True))
}
```

Output: `True`.

### Generic list append

`examples/list.neo`:

```neo
fn append[T: Type](xs: List[T], ys: List[T]): List[T] {
  match xs {
    Nil => ys
    Cons(h, t) => Cons(h, append(t, ys))
  }
}

fn range(n: Nat): List[Nat] {
  match n {
    Zero => Nil
    Succ(p) => Cons(p, range(p))
  }
}

fn main(): IO[List[Nat]] {
  IO::pure(append(range(3), range(2)))
}
```

Output: `Cons(2, Cons(1, Cons(0, Cons(1, Cons(0, Nil)))))`.

### Closures

`examples/closures.neo`:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(p) => Succ(add(p, b))
  }
}

fn addn(n: Nat): fn(x: Nat) -> Nat {
  fn(x: Nat) => add(n, x)
}

fn twice(f: fn(x: Nat) -> Nat, x: Nat): Nat {
  f(f(x))
}

fn main(): IO[Nat] {
  IO::pure(twice(addn(2), 3))
}
```

Output: `7`.

### User-defined enum

`examples/enums.neo` defines `Shape`, then matches it (using the `mul`
defined at the top of that file):

```neo
enum Shape {
  Circle(Nat)
  Rect(Nat, Nat)
}

fn area(s: Shape): Nat {
  match s {
    Circle(r) => mul(r, r)
    Rect(w, h) => mul(w, h)
  }
}
```

With `main` returning `IO::pure(area(Rect(3, 4)))`, the output is `12`.

### Equality proofs

`examples/proofs.neo` (which also defines `add` and `sym`):

```neo
fn add_zero(a: Nat): add(a, Zero) = a {
  match a {
    Zero => Refl
    Succ(p) => match sym(add(p, Zero), p, add_zero(p)) {
      Refl => Refl
    }
  }
}

fn main(): IO[Nat] {
  let proof = add_zero(3)
  IO::pure(7)
}
```

Output: `7`. The proof is checked and erased.

### Indexed families

`examples/indexed_vec.neo`:

```neo
enum Vec[T: Type, n: Nat] {
  Vnil : Vec[T, Zero]
  Vcons(m: Nat, x: T, xs: Vec[T, m]) : Vec[T, Succ(m)]
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    Zero => b
    Succ(k) => Succ(add(k, b))
  }
}

fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    Vnil => Zero
    Vcons(m, x, xs) => Succ(length(xs))
  }
}

fn append[T: Type, n: Nat, m: Nat](xs: Vec[T, n], ys: Vec[T, m]): Vec[T, add(n, m)] {
  match xs {
    Vnil => ys
    Vcons(k, x, tail) => Vcons(add(k, m), x, append(tail, ys))
  }
}

fn two(): Vec[Nat, 2] {
  Vcons(1, 4, Vcons(0, 3, Vnil))
}

fn three(): Vec[Nat, 3] {
  append(two(), Vcons(0, 7, Vnil))
}

fn main(): IO[Nat] {
  IO::pure(length(three()))
}
```

The REPL can display `two()` as `Vcons(1, 4, Vcons(0, 3, Vnil))`. The program
builds because `main`'s action produces `Nat`; a `main` producing `Vec` itself
is not printable. `length` matches on its implicit index `n`; refinement learns `n` is
`Zero` or `Succ(m)` in the two branches, which is what makes the recursive call
`length(xs)` well typed. `append` learns `n := Succ(k)` in its `Vcons` branch.
Output: `3`.

`examples/indexed_match.neo` shows branch-field refinement:

```neo
fn head[T: Type, n: Nat](v: Vec[T, Succ(n)]): T {
  match v {
    Vcons(m, x, xs) => x
  }
}

fn tail[T: Type, n: Nat](v: Vec[T, Succ(n)]): Vec[T, n] {
  match v {
    Vcons(m, x, xs) => xs
  }
}
```

The `Vnil` constructor is impossible for the scrutinee type, so it is omitted.
The `m` field is refined to `n`, which gives `xs` the type `Vec[Nat, n]`. With
`main` returning `IO::pure(head(Vcons(1, (4 : Nat), Vcons(0, (3 : Nat), Vnil))))`,
the output is `4`. The ascriptions are needed because `head`'s type parameters are
inferred, so the elements have no expected type of their own.

`examples/pattern_matching.neo` shows nested patterns and multiple arms for one
constructor:

```neo
enum Tree {
  Leaf
  Node(Tree, Tree)
}

fn count(t: Tree): Nat {
  match t {
    Leaf => Zero
    Node(Leaf, r) => Succ(count(r))
    Node(Node(a, b), r) => Succ(add(Succ(add(count(a), count(b))), count(r)))
  }
}
```

The nested `Node(a, b)` pattern exposes the inner subtree's fields, and
recursion on `a`, `b`, and `r` is accepted because all three are structural
subterms of `t`. Output: `4`.

`examples/argument_elision.neo` shows inferred implicit arguments:

```neo
fn length[T: Type, n: Nat](v: Vec[T, n]): Nat {
  match v {
    Vnil => Zero
    Vcons(m, x, xs) => Succ(length(xs))
  }
}

fn append[T: Type, n: Nat, m: Nat](xs: Vec[T, n], ys: Vec[T, m]): Vec[T, add(n, m)] {
  match xs {
    Vnil => ys
    Vcons(k, x, tail) => Vcons(add(k, m), x, append(tail, ys))
  }
}

fn main(): IO[Nat] {
  IO::pure(length(append(v1(), v2())))
}
```

The recursive calls `length(xs)` and `append(tail, ys)` omit their implicit
arguments. The outer calls need no annotations either: the types of `v1()` and
`v2()` determine the arguments to `append`, and the expected `Nat` plus the
remaining arguments determine the arguments to `length`. Output: `3`.

`examples/strings.neo` slices and concatenates strings:

```neo
fn main(): IO[String] {
  IO::pure("hello, world".slice(7, 12).concat("!"))
}
```

Both the interpreter and the generated WASI component print the result as
text, followed by a newline. Output:

```
world!
```

`examples/bytes.neo` builds a byte sequence:

```neo
fn main(): IO[Bytes] {
  IO::pure(b"neo".concat(b"\x00\xff"))
}
```

Byte results print as escaped bytes. Output:

```
b"\x6e\x65\x6f\x00\xff"
```

`examples/methods.neo` declares methods on `Bool` and a user enum:

```neo
impl Bool {
  pub fn not(self: Bool): Bool {
    match self {
      False => True
      True => False
    }
  }
}

enum Shape {
  Circle(U32)
  Rect(U32, U32)
}

impl Shape {
  pub fn area(self: Shape): U32 {
    match self {
      Circle(r) => r * r * 3u32
      Rect(w, h) => w * h
    }
  }
}
```

`Rect(3u32, 4u32).area()` is `12`. A method's name is qualified by its
receiver type, so `String.length`, `Bytes.length`, and a user `List.length` may
all exist at once.

---

## 19. Coming from other languages

| Other syntax | Neo |
| --- | --- |
| `def f(x) = e` | `fn f(x: T): U { e }` |
| `let x = e; body` | `{ let x = e`<br>`body }` |
| `\x -> e` or `lambda x: e` | `fn(x: T) => e` |
| `case e of ...` | `match e { ... }` |
| `forall (x: T), P x` | `fn(x: T) -> P(x)` |
| `theorem name: P := proof` | `fn name(): P { proof }` |
| `induction n` | recursive `match n { ... }` |
| `if b then x else y` | `match b {`<br>`True => x`<br>`False => y`<br>`}` |
| `f(x); g(y)` | one expression per tail; use `let` bindings |

There are no semicolons, no mutable variables, and no tactics.
