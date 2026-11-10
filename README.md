# Neo

Neo is a dependently typed, total programming language with a small trusted
proof kernel. It uses an ASCII, math-like syntax and compiles to WebAssembly
components that run on WASI 0.2.

This repository contains **Stage 0**: a Rust interpreter and WASI 0.2 backend.
Later stages will self-host the compiler in Neo.

## Status

Implemented:

- `enum` and `fn` declarations, `match`, equality and `refl`;
- dependent function types and explicit type parameters;
- closures and first-class functions;
- total recursive functions with a structural termination checker;
- strict positivity checking;
- a small prelude (`Nat`, `Bool`, `Unit`, `List`, `True`, `False`, `And`,
  `Or`, `Exists`);
- `neo` REPL, `neo FILE` interpretation, and `neo build FILE` to a WASI 0.2
  component.

Not implemented yet: references/ownership (`&T`, `&mut T`, `own T`), arrays
and growable vectors (`Array(T, n)`, `Vec(T)`), modules, strings, general I/O,
and built-in arithmetic. See the
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
fn main(): Nat {
  5
}
```

There are no string literals or I/O functions yet. A program reports its
result by returning it from `main`; the runtime prints that value.

More examples live in [`examples/`](examples):

| File | Demonstrates | Output |
| --- | --- | --- |
| `hello.neo` | minimal program | `5` |
| `nat.neo` | structural recursion | `42` |
| `bool.neo` | booleans, `match` | `true` |
| `list.neo` | generics, lists | `cons(2, cons(1, cons(0, cons(1, cons(0, nil)))))` |
| `closures.neo` | closures, first-class functions | `7` |
| `enums.neo` | user enums and patterns | `12` |
| `proofs.neo` | equality proofs, proof erasure | `7` |
| `indexed_vec.neo` | indexed families, length, append | `3` |
| `indexed_match.neo` | branch-field index refinement | `4` |
| `indexed_append.neo` | context index refinement in `append` | `3` |

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
