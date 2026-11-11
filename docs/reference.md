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
fn main(): Nat {
  5
}
```

Save it as `hello.neo`, then:

```sh
neo hello.neo                      # interpret: prints 5
neo build hello.neo -o hello.wasm  # compile to a WASI 0.2 component
wasmtime hello.wasm                # runs the component: prints 5
```

`neo build` writes `hello.wasm` next to the input by default.

There are no string literals or I/O functions yet. A program communicates its
result by returning it from `main`; Neo prints that value.

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
| `neo FILE` | Parses, checks, evaluates `main`, and prints its result. |
| `neo build FILE` | Compiles `main` and everything reachable from it into a component. |
| `neo build FILE -o PATH` | Same, with an explicit output path. `--output PATH` is also accepted. |
| `neo run` / `neo check` | Reserved and rejected with an error. |

Details:

- `neo FILE` requires `fn main(): T`; interpreting a library-only file is an
  error.
- `neo build` also requires `main`, and `T` must be a printable data type
  (see [section 13](#13-printing-and-building-for-wasi)).
- `neo build` accepts exactly one input file. Unknown `-` options are errors.
- Success exits with status 0. Any error is written to stderr and exits with
  status 1.
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
fn main(): Nat { 5 }  // So is this.
```

There are no block comments. There are no semicolons.

### Newlines

Newlines are significant. They separate:

- enum constructors,
- block bindings,
- match arms.

A newline **inside parentheses** is ignored, so argument lists, parameter
lists, and constructor fields may span lines:

```neo
fn add(
  a: Nat,
  b: Nat
): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}
```

Newlines inside `{ ... }` are not ignored.

### Identifiers

An identifier starts with an ASCII letter or `_` and continues with ASCII
letters, digits, `_`, or `'`:

```
x   foo   Nat   succ'   _tmp   x1
```

The single token `_` is the wildcard, not an identifier.

### Keywords

```
enum   fn   match   Type   Prop
```

`refl` is recognised specially in expressions. Everything else — including
`true`, `false`, `zero`, `nil`, and `unit` — is an ordinary name resolved from
the prelude or the program.

### Numeric literals

Numeric literals are unsigned decimal integers:

```neo
0   5   42   1000
```

A literal is syntactic sugar for a `Nat` built from `zero` and `succ`. There
are no negative, floating-point, hexadecimal, or character literals.

### Operators and punctuation

```
(  )  {  }  ,  :  =  ==  ->  =>
```

`->` is function-type arrow; `=>` introduces a lambda body or match arm.
`-` is not valid outside `->`.

---

## 4. Declarations

A program is a sequence of top-level declarations:

```neo
enum Name { ... }
fn name(...): Result { ... }
```

Rules:

- Only `enum` and `fn` may appear at the top level.
- Every name lives in one global namespace. Reusing a name is an error, and
  prelude names are already taken.
- A name must be declared before it is used. Forward references are errors.
- The only recursion available is a function calling **itself**. Mutual
  recursion is not supported.
- Definitions (`enum`, `fn`) cannot be nested. Function bodies are blocks,
  which may contain bindings and expressions.

For example, `add` must precede `mul`:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}

fn mul(a: Nat, b: Nat): Nat {
  match a {
    zero => zero
    succ(p) => add(b, mul(p, b))
  }
}
```

Because prelude constructors already occupy names, this is an error:

```neo
fn and(a: Bool, b: Bool): Bool { ... }  // `and` is already defined
```

The prelude's `and` is the constructor of `And(P, Q)`. Use another name such
as `both`.

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
List(Nat)
Vec(Nat, 2)
```

Type arguments in type expressions are explicit. Function calls may omit a
leading prefix of their arguments; see [argument elision](#argument-elision).
There are no declared implicit binders.

### Equality types

```neo
zero == zero
add(2, 3) == 5
```

`==` forms a proposition. Both sides must have the same type. See
[section 9](#9-equality-and-proofs).

### Universe rules

- `Prop` is impredicative: `fn(x: A) -> Prop` is itself a `Prop`, regardless
  of the size of `A`.
- `Type` is predicative: for a non-`Prop` codomain, a function type lives in
  `Type (max u v)` when its domain and codomain live in `Type u` and `Type v`.
- `Type u` is cumulative: a value of `Type u` may be used where `Type v` is
  expected when `u <= v`.
- `Prop` is not a subtype of `Type 0`; `Prop` is a **term** of `Type 0`.

There is no annotation syntax like `(e : T)`. Expected types come from
declarations, function parameters, constructors, and the surrounding context.

---

## 6. Enums

An enum declares an inductive family:

```neo
enum Nat {
  zero
  succ(Nat)
}
```

Syntax:

```
enum Name(parameters) : Sort {
  constructor(fields)
  constructor(fields) : ResultType
}
```

- The parameter list is optional.
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
enum Dep(A: Type, x: A) {
  mk(y: A)
}
```

Constructors take the enum parameters first, then their fields. Parameterised
constructors cannot be inferred from their arguments alone. Given:

```neo
enum Maybe(T: Type) {
  none
  some(T)
}
```

this is an error:

```neo
fn bad(): Nat {
  m = some(5)     // cannot infer the type arguments of `some`
  0
}
```

The expected type must be known:

```neo
fn five(): Maybe(Nat) {
  some(5)
}

fn main(): Nat {
  m = five()
  0
}
```

### Fields

Fields are positional. A field may be named:

```neo
enum Pair {
  pair(Nat, Bool)          // two unnamed fields
}

enum Sigma(A: Type, B: fn(x: A) -> Type) {
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
enum Vec(T: Type, n: Nat) {
  vnil : Vec(T, zero)
  vcons(m: Nat, x: T, xs: Vec(T, m)) : Vec(T, succ(m))
}

fn two(): Vec(Nat, 2) {
  vcons(1, 4, vcons(0, 3, vnil))
}
```

Here `m` is an ordinary runtime field that records the length of the tail,
and the constructor's result type connects it to `succ(m)`. Because `Vec` has
parameters, `vcons(...)` must be used where the expected type is known.

Indexed families support **index refinement**. Matching refines both constructor
fields and ordinary context parameters that the scrutinee's indices determine.
Constructors whose result indices cannot match become impossible branches:

```neo
enum Vec(T: Type, n: Nat) {
  vnil : Vec(T, zero)
  vcons(m: Nat, x: T, xs: Vec(T, m)) : Vec(T, succ(m))
}

fn tail(n: Nat, v: Vec(Nat, succ(n))): Vec(Nat, n) {
  match v {
    vcons(m, x, xs) => xs
  }
}
```

The scrutinee `Vec(Nat, succ(n))` cannot be built by `vnil`, so the `vnil` arm
may be omitted. In the `vcons` branch, the equation `succ(m) == succ(n)`
refines the field `m` to `n`, so `xs` has type `Vec(Nat, n)`. An arm written for
an impossible constructor is accepted but never checked, and the generated code
omits it.

Context parameters are refined too. The classic length and append functions
work directly on a bare index:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn length(n: Nat, v: Vec(Nat, n)): Nat {
  match v {
    vnil => zero
    vcons(m, x, xs) => succ(length(m, xs))
  }
}

fn append(n: Nat, m: Nat, xs: Vec(Nat, n), ys: Vec(Nat, m)): Vec(Nat, add(n, m)) {
  match xs {
    vnil => ys
    vcons(k, x, tail) => vcons(add(k, m), x, append(k, m, tail, ys))
  }
}
```

In the `vnil` branch of `length`, the equation `n == zero` refines `n`; in the
`vcons` branch it refines `n` to `succ(m)`. In `append`, the `vcons` branch
learns `n == succ(k)`, so the constructed vector has index
`succ(add(k, m))`, convertible to `add(succ(k), m)`.

Indexed families are still limited:

- Refinement is first-order. An index that is stuck, such as `pred(n)` where
  `pred` is an ordinary recursive function, cannot be related to a constructor
  and the match is rejected. A cyclic equation is rejected too.
- A value whose type has a field like `Vec(T, m)` that remains dependent on an
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
fn name(parameter, ...): ResultType {
  binding
  ...
  finalExpression
}
```

Example:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}
```

- Parentheses are always required, including for zero parameters:
  `fn main(): Nat { ... }`.
- The body is always a block and must end in an expression. There is no
  `return` keyword.
- Parameter types and the result type may depend on earlier parameters:

```neo
fn replicate(T: Type, n: Nat, x: T): List(T) {
  match n {
    zero => nil
    succ(p) => cons(x, replicate(T, p, x))
  }
}
```

### Blocks and bindings

A block is `{ ... }`. It may start with any number of bindings, each on its
own line, followed by exactly one tail expression:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}

// `add` is now in scope.
fn main(): Nat {
  x = 2
  y = 3
  add(x, y)
}
```

- `name = expression` introduces a binding. There is no `let`.
- Bindings are sequential and may shadow earlier names.
- Bindings cannot be annotated; their types are inferred.
- The value of the block is the value of its final expression.
- A block is itself an expression, so it can appear anywhere an expression is
  allowed.

Because bindings are inferred, a parameterised constructor in a binding needs
a helper function with a known return type:

```neo
fn main(): Nat {
  m = cons(5, nil)   // error: cannot infer the type arguments of `cons`
  0
}
```

This works instead:

```neo
fn five(): List(Nat) {
  cons(5, nil)
}

fn main(): Nat {
  m = five()
  0
}
```

### Application

Application uses parentheses and commas:

```neo
add(2, 3)
f(x, y, z)
id(Nat, 4)          // type arguments are explicit
```

- Application is left-associative: `f(a)(b)` applies `f` to `a`, then to `b`.
- An empty argument list `f()` is accepted and is equivalent to `f`.
- Type parameters are ordinary parameters; by convention they come first.

### Argument elision

A call may omit a **leading prefix** of the function's parameters. The
elaborator fills those arguments with metavariables and solves them from the
types of the supplied arguments and from the expected result type:

```neo
fn length(n: Nat, v: Vec(Nat, n)): Nat

length(v)                 // n inferred from v
append(xs, ys)            // both lengths inferred
map(inc, xs)              // type parameters inferred from inc and xs
```

Declarations do not change, and there are no implicit binder markers.
Explicit calls keep working:

```neo
length(2, v)
append(2, 1, xs, ys)
```

Rules:

- Elision applies only to a leading prefix. A middle argument cannot be
  skipped.
- The ordinary, more explicit interpretation is tried first. Elision is used
  only when that interpretation does not typecheck.
- Every omitted argument must be determined by the remaining arguments or the
  expected type. Otherwise the program is rejected and the argument must be
  passed explicitly.
- Only first-order solutions are attempted. An omitted argument whose value
  would be an applied function is rejected.
- Elision is elaboration sugar. Core terms still contain every argument, so the
  kernel and code generator are unchanged.

There is no infix arithmetic. Define functions such as `add` and `mul`
yourself.

### Function types

```neo
fn(x: Nat) -> Nat
fn(T: Type, x: T) -> T
```

A binder name may be omitted: `fn(Nat) -> Nat`.

### Lambdas

```neo
fn(x: Nat) => succ(x)
fn(a: Type, x: a) => x
fn(Nat) => zero
```

Lambdas capture their free variables by value. They are first-class values:

```neo
fn apply(f: fn(x: Nat) -> Nat, x: Nat): Nat {
  f(x)
}

fn main(): Nat {
  apply(fn(y: Nat) => succ(y), 3)
}
```

Lambdas are for parameters. A zero-argument function is written as a
declaration:

```neo
fn zero(): Nat {
  0
}
```

(`fn() => e` has no useful meaning and is not the way to write one.)

### Parentheses

Parentheses group expressions:

```neo
(fn(x: Nat) => succ(x))(3)
```

### Numeric literals

A numeral is a `Nat`:

```neo
fn main(): Nat {
  42
}
```

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
    zero => zero
    succ(p) => p
  }
}
```

### Patterns

```
_                        wildcard
x                        variable binder
zero                     nullary constructor
succ(p)                  constructor with sub-patterns
cons(_, t)               wildcard field
cons(succ(k), rest)      nested constructor pattern
```

Patterns nest arbitrarily. A nested pattern compiles to an inner `match` on the
field, using the same index refinement as an ordinary match:

```neo
fn classify(n: Nat): Nat {
  match n {
    zero => zero
    succ(zero) => 1
    succ(succ(zero)) => 2
    succ(succ(succ(k))) => 3
  }
}
```

Multiple arms for one constructor are allowed and checked in order:

```neo
fn small(xs: List(Nat)): Nat {
  match xs {
    nil => zero
    cons(zero, rest) => zero
    cons(succ(k), rest) => k
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
  the constructor, not as a binder. For example, a lone `zero => ...` arm is a
  constructor pattern for `Nat`, and the match is then rejected as
  non-exhaustive. Use `_` or a different name for an irrefutable arm.

### Dependent results

If the expected result type is known and the scrutinee is a variable, the
result type is abstracted over that variable, so it may mention it. Indices are
refined from the scrutinee type (see
[indexed families](#indexed-families)): solving `succ(m) = succ(n)` gives the
branch field `m` the value `n`, and field types that mention `m` are rewritten
accordingly. Solving `succ(m) = n` refines the context parameter `n` to
`succ(m)`, so later branches and recursive calls see the refined type.

Refinement is first-order and total. It does not guess: a stuck index such as
`pred(n)`, or an equation that would make an index depend on itself, is
rejected instead of being assumed.

### Matching on equality proofs

A match on a proof of `x == y` is special. It takes exactly one arm, whose
pattern is `refl` or `_`:

```neo
fn sym(T: Type, x: T, y: T, p: x == y): y == x {
  match p {
    refl => refl
  }
}
```

The arm is checked against a motive. If the expected type is written in terms
of the right-hand side `y`, that occurrence is abstracted to the motive
argument; the `refl` arm is then checked at the left-hand side `x`, and the
whole match has the expected type with `y` in place. This is motive-based
equality elimination (`J`), not textual replacement.
See [section 9](#9-equality-and-proofs).

Abstraction is capture-avoiding, so it works under binders. For example, an
expected type that mentions `x` inside a function type is abstracted too:

```neo
fn h(x: Nat, p: x == zero): fn(y: Nat) -> zero == x {
  match p {
    refl => fn(y: Nat) => refl
  }
}
```

A `match` whose scrutinee is a compound expression is abstracted the same
way: the motive replaces every occurrence of that expression. Abstraction is
syntactic, so an occurrence that is merely convertible to the scrutinee is
not found; that case needs an explicit helper.

---

## 9. Equality and proofs

`==` is a built-in homogeneous equality proposition:

```neo
zero == zero
add(2, 3) == 5
```

The type of the left side determines the type of the right side; both must be
the same type.

### `refl`

`refl` proves `x == y` when `x` and `y` are definitionally equal. It is only
valid where the expected type is a known equality:

```neo
fn refl_zero(): zero == zero {
  refl
}
```

Writing `refl` in an inference position is an error:

```
`refl` needs an expected equality type; use it where the type is known
```

If the sides are not convertible:

```
`refl` cannot prove `zero == succ(zero)` (sides are not equal)
```

### Proving with recursive matches

Proofs are ordinary total functions. Recursive `match` replaces tactics such
as `induction` (using the `sym` defined above):

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}

fn add_zero(a: Nat): add(a, zero) == a {
  match a {
    zero => refl
    succ(p) => match sym(Nat, add(p, zero), p, add_zero(p)) {
      refl => refl
    }
  }
}
```

There are no tactics, no `theorem` keyword, and no `forall`. A theorem is a
`fn` returning a proposition.

### What can be eliminated

- Equality proofs (`match p { refl => ... }`) may produce any type, including
  computational data. The proof is erased at run time.
- A proposition enum (`And`, `Or`, `Exists`, `True`) is intended for
  proof-level computation only. The kernel enforces this: eliminating such a
  value into a computational type is rejected (large elimination).
- The exception is the *singleton* rule. A `Prop` enum with no constructors
  (`False`), or with exactly one constructor whose fields are all propositions
  (for example `x == y` with its `refl` constructor), may be eliminated into any
  type. This is what makes equality proofs usable as ordinary values.
- `False`, which has no constructors, is eliminated with a zero-arm match.
  The expected result type must be known.

### Proof erasure

At run time, types and type-level terms carry no information. Equality proofs
and `J` compile to their `refl` case. A proof can be bound and ignored:

```neo
fn main(): Nat {
  proof = add_zero(3)
  7
}
```

Proposition-enum values such as `trivial` are ordinary runtime values at
Stage 0; this is why `fn main(): True { trivial }` can print `trivial`.

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
    zero => b
    succ(p) => succ(add(p, b))
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
  bad2(succ(n))             // increases
}

fn bad3(n: Nat): Nat {
  match n {
    zero => zero
    succ(p) => bad3(succ(p))  // non-variable expression containing a smaller value
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

- may occur **directly**: `T`, `List(T)`, `Vec(T, n)`;
- may not occur to the left of an arrow: `fn(x: T) -> Nat`;
- may not occur as an argument of another type constructor:
  `List(T)` inside `T`'s own definition is a nested inductive and is rejected;
- may not be hidden inside an application or match.

Rejected:

```neo
enum Bad {
  bad(List(Bad))            // nested inductive
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
enum Bad(F: Type 1) : Type 1 {
  mk(g: F) : Bad(fn(x: Bad(F)) -> Type 0)
}
```

```
`Bad` occurs in a constructor result index; enum indices may not mention the enum being defined
```

```neo
enum E(A: Type 1) : Type 0 {
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
enum E(A: Type 1) : Type 0 {
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
```

Notes:

- `Nat` numerals desugar to `zero` and `succ`.
- `List` is `List(T)`, not a built-in vector; it is a linked list.
- `And`, `Or`, `Exists`, `True`, and `False` are propositions (`Prop`).
- Constructor names are global. Avoid `and` if you also want a Boolean
  conjunction function; name it `both` instead.
- There is no built-in arithmetic. Define `add`, `mul`, and so on.

---

## 13. Printing and building for WASI

### Requirements for `main`

`main` must be a `fn` declaration with **no parameters**:

```neo
fn main(): Nat {
  5
}
```

Its result type must be printable.

A type is printable when it is:

- `Nat`, `Bool`, or `Unit`;
- an enum whose type arguments are printable and whose constructor field types
  are all printable.

This includes ordinary parameterised enums such as `List(Nat)` and a
user-defined `Maybe(Nat)`, and proposition enums with printable fields such as
`True`. The following are **not** printable:

- function types;
- `Type` and `Prop`;
- equality types such as `x == y`;
- enums with a field whose type is a type family applied to an earlier field,
  such as `Vec(T, m)` in the indexed `Vec` example.

A program whose `main` result is not printable is rejected by both the
interpreter and `neo build`:

```
the result type of `main` is not printable: `Vec(Nat, 2)`
```

A `main` returning a function type is currently reported as
`` `main` must not take parameters ``; either way, it is not buildable.

### Printed form

Values print with surface constructor names and without type parameters:

```neo
cons(2, cons(1, cons(0, nil)))     // List(Nat)
cons(5, nil)                       // List(Nat)
pair(4, true)
unit
true
42                                 // Nat numerals are printed in decimal
trivial                            // True
```

Index fields declared as normal fields are printed too. The `Vec` example
prints `vcons(1, 4, vcons(0, 3, vnil))`, where `1` is the recorded tail
length.

The runtime prints one newline after the value.

### Generated component

`neo build` produces a **WASI 0.2 component**, not a core module:

- export: `wasi:cli/run@0.2.9#run`;
- imports: `wasi:cli/stdout@0.2.9` and `wasi:io/streams@0.2.9` for printing;
- memory is exported;
- the program returns exit code 0 after printing.

Run it with any WASI 0.2 runtime:

```sh
wasmtime program.wasm
```

Only functions reachable from `main` are compiled. Unused proofs and type-level
functions do not affect code generation. Types and type-level terms are erased;
equality proofs and `J` reduce to their `refl` case. Proposition-enum values
remain ordinary values and can be printed. The runtime allocates with a bump
allocator; memory grows on demand. There is no garbage collector and no
reference counting.

There is no user-facing I/O API yet: a program cannot read input, open files,
or print custom strings.

---

## 14. The REPL

Run `neo` with no arguments:

```
neo 0.1.0 (type :help for help)
neo>
```

The REPL accepts two kinds of input:

1. **Declarations** starting with `enum ` or `fn <name>(`. They are added to
   the current environment:
   ```neo
   neo> fn inc(x: Nat): Nat { succ(x) }
   ok
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
- equality proofs (for example, a proof of `2 == 2` cannot be displayed);
- bare type constructors other than `Nat`, e.g. `List(Nat)`.

REPL errors are printed and the session continues. The REPL does not require a
`main` function.

---

## 15. Grammar

A simplified EBNF. `NEWLINE` is significant outside parentheses.

```ebnf
program      ::= { NEWLINE } { decl { NEWLINE } }

decl         ::= enum_decl | fn_decl

enum_decl    ::= "enum" IDENT [ "(" params ")" ]
                 [ ":" expr ]
                 "{" { NEWLINE } { ctor NEWLINE } "}"
ctor         ::= IDENT [ "(" fields ")" ] [ ":" expr ]

fn_decl      ::= "fn" IDENT "(" params ")" ":" expr block

params       ::= [ param { "," param } ]
param        ::= IDENT ":" expr

fields       ::= [ field { "," field } ]
field        ::= [ IDENT ":" ] expr

block        ::= "{" { NEWLINE } { binding NEWLINE } expr { NEWLINE } "}"
binding      ::= IDENT "=" expr

expr         ::= fn_expr | eq_expr

fn_expr      ::= "fn" "(" binders ")" ( "->" expr | "=>" expr )
binders      ::= [ binder { "," binder } ]
binder       ::= [ IDENT ":" ] expr

eq_expr      ::= app { "==" app }
app          ::= atom { "(" args ")" }
args         ::= [ expr { "," expr } ]

atom         ::= IDENT
               | NAT
               | "Type" [ NAT ]
               | "Prop"
               | "(" expr ")"
               | block
               | match_expr
               | fn_expr

match_expr   ::= "match" expr "{" { NEWLINE } { arm NEWLINE } "}"
arm          ::= pattern "=>" expr

pattern      ::= "_"
               | IDENT
               | IDENT "(" [ pattern { "," pattern } ] ")"
```

Notes:

- `IDENT` excludes the keywords and `_`.
- A final constructor or match arm may be followed by `}` instead of a
  newline.
- A bare `refl` name is parsed as the `refl` expression.
- A lone identifier in a pattern is a binder unless it names a nullary
  constructor of the scrutinee's enum.

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
| `unknown name ...` | Name is undeclared or declared later. |
| `type mismatch: expected ..., found ...` | Expression has the wrong type. |
| `cannot infer the type arguments of ...; use it where the expected type is known` | Parameterised constructor in an inference position. |
| `cannot infer omitted argument of type ...; pass it explicitly` | An elided leading argument is not determined by the rest of the call. |
| `match is not exhaustive: missing constructor ...` | Add the missing arm. |
| `cannot infer the result type of a match on an empty enum; add an expected type` | Empty enums are eliminated with zero arms, but need a known result type. |
| `match: cannot eliminate this proposition into a type` | Large elimination of a `Prop` enum was attempted. |
| `... is larger than its enum` / `... field lives in ...` | A `Type u` enum has a constructor field or index in a larger sort. |
| `... occurs in a constructor result index ...` | The enum appears in a constructor result index. |
| `recursive function ... is used as a value` | The recursive name escaped; only fully applied calls are allowed. |
| `recursive call to ... is not structurally decreasing ...` | A recursive call does not descend on a subterm of one fixed parameter. |
| `constructor ... must return ...` | A constructor's explicit result type must be its declaring enum. |
| `a match on an equality proof must use the pattern refl` | Equality matches have one `refl`/`_` arm. |
| `recursive call ... is not structurally decreasing ...` | Termination check failed. |
| `... is not strictly positive ...` | Positivity check failed. |
| `this program needs fn main(): T` | The file has no `main`. |
| `main must not take parameters` | `main` was declared with parameters, or returns a function type. |
| `the result type of main is not printable: ...` | Pick a printable result type. |

---

## 17. Limitations and not-yet-supported

The following are **not** part of Stage 0, even if they appear in the long-term
design:

- references and ownership modes: `&T`, `&mut T`, `own T`;
- arrays and growable vectors: the planned `Array(T, n)` and `Vec(T)`. (The
  indexed `Vec` in [section 6](#6-enums) is a user-defined demonstration, not
  the planned growable vector.)
- declared implicit binders, traits, or type classes. Leading arguments can be
  inferred at calls; see [argument elision](#argument-elision);
- modules, imports, namespaces, or multiple files;
- strings, characters, floats, and general I/O;
- built-in arithmetic or other infix operators;
- mutual recursion and mutual inductive types;
- nested or higher-order inductive occurrences;
- higher-order or non-first-order dependent pattern matching: indices are
  refined only when they unify structurally with a constructor result. Context
  and field refinement are supported (see [section 6](#6-enums)); a stuck index
  such as `pred(n)` is rejected;
- proof irrelevance in the conversion checker;
- custom printing and user-defined entry points;
- `def`, `let`, `<-`, mutable variables, `proc`, `theorem`, `forall`, `lambda`,
  `case`, and `induction`. Use `fn`, block bindings, `fn(...) => ...`, and
  `match` instead.

`main` result types with a field like `Vec(T, m)` that stays dependent on an
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
    zero => b
    succ(p) => succ(add(p, b))
  }
}

fn mul(a: Nat, b: Nat): Nat {
  match a {
    zero => zero
    succ(p) => add(b, mul(p, b))
  }
}

fn main(): Nat {
  mul(6, 7)
}
```

Output: `42`.

### Booleans and `match`

`examples/bool.neo`:

```neo
fn not(b: Bool): Bool {
  match b {
    true => false
    false => true
  }
}

fn both(a: Bool, b: Bool): Bool {
  match a {
    true => b
    false => false
  }
}

fn main(): Bool {
  both(not(false), true)
}
```

Output: `true`.

### Generic list append

`examples/list.neo`:

```neo
fn append(T: Type, xs: List(T), ys: List(T)): List(T) {
  match xs {
    nil => ys
    cons(h, t) => cons(h, append(T, t, ys))
  }
}

fn range(n: Nat): List(Nat) {
  match n {
    zero => nil
    succ(p) => cons(p, range(p))
  }
}

fn main(): List(Nat) {
  append(Nat, range(3), range(2))
}
```

Output: `cons(2, cons(1, cons(0, cons(1, cons(0, nil)))))`.

### Closures

`examples/closures.neo`:

```neo
fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(p) => succ(add(p, b))
  }
}

fn addn(n: Nat): fn(x: Nat) -> Nat {
  fn(x: Nat) => add(n, x)
}

fn twice(f: fn(x: Nat) -> Nat, x: Nat): Nat {
  f(f(x))
}

fn main(): Nat {
  twice(addn(2), 3)
}
```

Output: `7`.

### User-defined enum

`examples/enums.neo` defines `Shape`, then matches it (using the `mul`
defined at the top of that file):

```neo
enum Shape {
  circle(Nat)
  rect(Nat, Nat)
}

fn area(s: Shape): Nat {
  match s {
    circle(r) => mul(r, r)
    rect(w, h) => mul(w, h)
  }
}
```

With `main` returning `area(rect(3, 4))`, the output is `12`.

### Equality proofs

`examples/proofs.neo` (which also defines `add` and `sym`):

```neo
fn add_zero(a: Nat): add(a, zero) == a {
  match a {
    zero => refl
    succ(p) => match sym(Nat, add(p, zero), p, add_zero(p)) {
      refl => refl
    }
  }
}

fn main(): Nat {
  proof = add_zero(3)
  7
}
```

Output: `7`. The proof is checked and erased.

### Indexed families

`examples/indexed_vec.neo`:

```neo
enum Vec(T: Type, n: Nat) {
  vnil : Vec(T, zero)
  vcons(m: Nat, x: T, xs: Vec(T, m)) : Vec(T, succ(m))
}

fn add(a: Nat, b: Nat): Nat {
  match a {
    zero => b
    succ(k) => succ(add(k, b))
  }
}

fn length(n: Nat, v: Vec(Nat, n)): Nat {
  match v {
    vnil => zero
    vcons(m, x, xs) => succ(length(m, xs))
  }
}

fn append(n: Nat, m: Nat, xs: Vec(Nat, n), ys: Vec(Nat, m)): Vec(Nat, add(n, m)) {
  match xs {
    vnil => ys
    vcons(k, x, tail) => vcons(add(k, m), x, append(k, m, tail, ys))
  }
}

fn two(): Vec(Nat, 2) {
  vcons(1, 4, vcons(0, 3, vnil))
}

fn three(): Vec(Nat, 3) {
  append(2, 1, two(), vcons(0, 7, vnil))
}

fn main(): Nat {
  length(3, three())
}
```

The REPL can display `two()` as `vcons(1, 4, vcons(0, 3, vnil))`. The program
builds because `main` returns `Nat`; a `main` returning `Vec` itself is not
printable. `length` matches on a bare index `n`; refinement learns `n` is `zero`
or `succ(m)` in the two branches, which is what makes the recursive call
`length(m, xs)` well typed. `append` learns `n := succ(k)` in its `vcons`
branch. Output: `3`.

`examples/indexed_match.neo` shows branch-field refinement:

```neo
fn head(n: Nat, v: Vec(Nat, succ(n))): Nat {
  match v {
    vcons(m, x, xs) => x
  }
}

fn tail(n: Nat, v: Vec(Nat, succ(n))): Vec(Nat, n) {
  match v {
    vcons(m, x, xs) => xs
  }
}
```

The `vnil` constructor is impossible for the scrutinee type, so it is omitted.
The `m` field is refined to `n`, which gives `xs` the type `Vec(Nat, n)`. With
`main` returning `head(1, vcons(1, 4, vcons(0, 3, vnil)))`, the output is `4`.

`examples/pattern_matching.neo` shows nested patterns and multiple arms for one
constructor:

```neo
enum Tree {
  leaf
  node(Tree, Tree)
}

fn count(t: Tree): Nat {
  match t {
    leaf => zero
    node(leaf, r) => succ(count(r))
    node(node(a, b), r) => succ(add(succ(add(count(a), count(b))), count(r)))
  }
}
```

The nested `node(a, b)` pattern exposes the inner subtree's fields, and
recursion on `a`, `b`, and `r` is accepted because all three are structural
subterms of `t`. Output: `4`.

`examples/argument_elision.neo` shows inferred leading arguments:

```neo
fn length(n: Nat, v: Vec(Nat, n)): Nat {
  match v {
    vnil => zero
    vcons(m, x, xs) => succ(length(xs))
  }
}

fn append(n: Nat, m: Nat, xs: Vec(Nat, n), ys: Vec(Nat, m)): Vec(Nat, add(n, m)) {
  match xs {
    vnil => ys
    vcons(k, x, tail) => vcons(add(k, m), x, append(tail, ys))
  }
}

fn main(): Nat {
  length(append(v1(), v2()))
}
```

The recursive calls `length(xs)` and `append(tail, ys)` omit their length
arguments. The outer calls need no annotations either: the lengths of `v1()`
and `v2()` determine the arguments to `append`, and the expected `Nat` plus the
remaining arguments determine the argument to `length`. Output: `3`.

---

## 19. Coming from other languages

| Other syntax | Neo |
| --- | --- |
| `def f(x) = e` | `fn f(x: T): U { e }` |
| `let x = e; body` | `{ x = e`<br>`body }` |
| `\x -> e` or `lambda x: e` | `fn(x: T) => e` |
| `case e of ...` | `match e { ... }` |
| `forall (x: T), P x` | `fn(x: T) -> P(x)` |
| `theorem name: P := proof` | `fn name(): P { proof }` |
| `induction n` | recursive `match n { ... }` |
| `if b then x else y` | `match b {`<br>`true => x`<br>`false => y`<br>`}` |
| `f(x); g(y)` | one expression per tail; use bindings |

There are no semicolons, no `let`, no mutable variables, and no tactics.
