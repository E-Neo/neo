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
| `indexed_vec.neo` | indexed families | `2` |

## Documentation

- [Language reference](docs/reference.md) — syntax, types, matching, proofs,
  termination, positivity, CLI, grammar, and limitations.
- [`wit/README.md`](wit/README.md) — provenance of the vendored WASI WIT files.

## Repository layout

```
src/        interpreter, elaborator, trusted kernel, and WASM backend
tests/      kernel tests, codegen integration tests, example checks
examples/   runnable .neo programs used by the documentation
docs/       language reference
wit/        root WIT world and vendored WASI 0.2.9 WIT dependencies
```

## How it works

The implementation is split into:

- a pure, total **logic core** with de Bruijn indices and normalization by
  evaluation;
- an **elaborator** that checks surface syntax and produces core terms;
- an untrusted **runtime core** that evaluates programs and generates WASM.

Types and equality proofs are erased during code generation; proposition-enum
values remain ordinary values. Values are represented as heap blocks; closures
use a uniform unary calling convention. The generated component imports only
the WASI output stream needed to print the result of `main`.

## License

No license has been chosen for this project yet.
