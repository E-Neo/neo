# Neo

Neo is a dependently typed, total programming language with a small trusted
proof kernel. It uses an ASCII, math-like syntax and compiles to WebAssembly
components that run on WASI 0.2.

This repository contains **Stage 0**: a Rust interpreter and WASI 0.2 backend.
Later stages will self-host the compiler in Neo. The bootstrap scope, decisions,
and milestones are in [`docs/bootstrap.md`](docs/bootstrap.md).

## Status

Implemented:

- `enum`, `fn`, `impl`, `mod`, and `use` items, `match`, equality, and `Refl`;
- immutable `let` bindings with optional annotations;
- expected-type-directed numerals and `(e : T)` ascription;
- infix arithmetic, bitwise, shift, comparison, and boolean equality over
  `Nat` and `U32`;
- dependent function types and implicit parameters in `[]`;
- implicit type and index parameters inferred at call sites;
- closures and first-class functions;
- total recursive functions with a structural termination checker;
- strict positivity checking;
- a small prelude (`Nat`, `Bool`, `Unit`, `List`, `True`, `False`, `And`,
  `Or`, `Exists`);
- a primitive 32-bit word type `U32` with total arithmetic, bitwise, and
  comparison operations, compiled to unboxed WebAssembly `i32`s;
- modules with nested items, `use` paths, aliases, and constructor globs;
  private-by-default visibility with `pub`;
- `impl` blocks and receiver-keyed methods: `impl Type { ... }` declarations,
  `x.name(args)` calls with static dispatch, generic receivers, and `pub`
  methods;
- primitive `String` (UTF-8) and `Bytes` with O(1) length and slicing,
  concatenation, equality, and printing, exposed as methods;
- prelude `Option` and `Result` types, and checked `Bytes.to_str` with a
  `Utf8Error` that reports the first invalid byte;
- host capabilities for standard error, the argument vector, standard input,
  and regular files (`stderr::write`, `env::argv`, `stdin::read_all`,
  `file::read`, `file::write`);
- `neo` REPL, `neo FILE` interpretation, and `neo build FILE` to a WASI 0.2
  component.

Not implemented yet: references/ownership (`&T`, `&mut T`, `own T`), arrays
and growable vectors (`Array[T, n]`, `Vec[T]`), multiple source files,
`pub use` re-exports, networking, and I/O beyond arguments, stdin, and regular
files. See the
[language reference](docs/reference.md#17-limitations-and-not-yet-supported)
for the complete list.

## Build

```sh
cargo build --release
cargo test
```

The binary is `target/release/neo`; the commands below assume it is on your
`PATH` (or use `cargo run -- ...`).

To build the compiler itself for WASI:

```sh
cargo build --target wasm32-wasip2
# target/wasm32-wasip2/debug/neo.wasm
```

The codegen tests execute generated components with `wasmtime` when it is
available and skip the execution assertion otherwise.

## Use

```sh
neo                                  # interactive REPL
neo examples/nat.neo                 # interpret a file
neo build examples/nat.neo -o nat.wasm
wasmtime nat.wasm                    # run the WASI 0.2 component
```

The smallest program:

```neo
fn main(): IO[Nat] {
  IO::pure(5)
}
```

`main` produces an `IO` action. `IO::pure(v)` produces `v`; `IO::bind(m, k)`
runs `m` and passes its result to `k`. Host I/O arrives through `extern`
capabilities inside modules; the prelude provides `stderr::write`. The runtime
runs `main`'s action once and prints its result, including strings.

More examples live in [`examples/`](examples):

| File | Demonstrates | Output |
| --- | --- | --- |
| `hello.neo` | minimal program | `5` |
| `io.neo` | `IO` actions and `IO::bind` | `42` |
| `stderr.neo` | `extern` host capability | `0` |
| `nat.neo` | structural recursion | `42` |
| `bool.neo` | booleans, `match` | `True` |
| `list.neo` | generics, lists | `Cons(2, Cons(1, Cons(0, Cons(1, Cons(0, Nil)))))` |
| `closures.neo` | closures, first-class functions | `7` |
| `enums.neo` | user enums and patterns | `12` |
| `proofs.neo` | equality proofs, proof erasure | `7` |
| `indexed_vec.neo` | indexed families, length, append | `3` |
| `indexed_match.neo` | branch-field index refinement | `4` |
| `indexed_append.neo` | context index refinement in `append` | `3` |
| `pattern_matching.neo` | nested patterns, tree recursion | `4` |
| `argument_elision.neo` | inferred implicit arguments | `3` |
| `strings.neo` | string literals and printing | `hello, world!` |

## Documentation

- [Language reference](docs/reference.md) — syntax, types, matching, proofs,
  termination, positivity, CLI, grammar, and limitations.
- [Core specification](docs/core-spec.md) — the trusted kernel calculus,
  typing and conversion rules, index refinement, and the trust boundary.
- [`crates/neo-codegen/wit/README.md`](crates/neo-codegen/wit/README.md) —
  provenance of the vendored WASI WIT files.

## Repository layout

```
crates/neo-common/    spans and diagnostics, shared by every crate
crates/neo-kernel/    the trusted proof kernel
crates/neo-frontend/  lexer, parser, surface syntax, elaborator, prelude
crates/neo-codegen/   WASI 0.2 component backend and its WIT files
src/                  the `neo` facade library and the `neo` CLI
tests/                kernel tests, codegen integration tests, example checks
examples/             runnable .neo programs used by the documentation
docs/                 language reference and trusted-core specification
```

Dependencies point one way:

```
neo-common <- neo-kernel <- neo-frontend
                  ^            ^
                  |            |
               neo-codegen ----+
                  ^
                  |
                 neo (facade + CLI)
```

`neo-kernel` never depends on the frontend, the code generator, or the syntax
crates. `neo-frontend` and `neo-codegen` never depend on each other.

## How it works

The implementation is split into:

- `neo-kernel`, the trusted proof kernel: a pure, total **logic core** with
  de Bruijn indices, normalization by evaluation, conversion, typing and
  checking, positivity, and termination;
- `neo-frontend`, the untrusted lexer, parser, surface syntax, and elaborator;
- `neo-codegen`, the untrusted **runtime core** that erases proofs and
  generates a WASI 0.2 component;
- `neo-common`, the shared spans and diagnostics;
- `neo`, the facade library and the `neo` command-line program.

Types and equality proofs are erased during code generation; proposition-enum
values remain ordinary values. Values are represented as heap blocks; closures
use a uniform unary calling convention. The generated component imports only
the WASI output stream needed to print the result of `main`.

## License

No license has been chosen for this project yet.
