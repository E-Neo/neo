# Self-hosting Neo

Neo is currently **Stage 0**: a Rust interpreter and WASI 0.2 backend. The goal
is a compiler written in Neo that compiles itself, with a byte-identical
fixpoint at Stage 3.

This document records the scope, the decisions that shape it, and the order of
work. Milestones **M0–M3** are implemented: the decision record, the
deterministic corpus oracle, the primitive `U32` layer, the surface-syntax
revision, and primitive `String`/`Bytes`.

## 1. Stages and gates

| Stage | Produced by | Gate |
| --- | --- | --- |
| Stage 0 | Rust (today) | Existing CI: fmt, clippy, tests, examples interpreted and built and run under `wasmtime` |
| Stage 1 | Neo source, compiled by Stage 0 | Builds the whole corpus with byte-identical output; builds itself for `wasm32-wasip2` and runs under `wasmtime` |
| Stage 2 | Stage 1 | Output equals Stage 1's output for the corpus |
| Stage 3 | Stage 2 | `sha256(Stage 2) == sha256(Stage 3)` for the compiler and every corpus program |

Byte-identity is required only between binaries produced by **Neo** compilers.
Stage 0 has to agree semantically (accept/reject, inferred types, runtime
behaviour), which is why the component emitter is canonicalised at M8 rather
than reverse-engineered.

Stage 1 must run as a `wasm32-wasip2` component, so file and argument I/O come
before the kernel port.

## 2. Decisions

These were reviewed and accepted. The numbers are stable references for
commits and design notes.

| # | Decision |
| --- | --- |
| D1 | **Trust boundary.** Stage 1 ports the full kernel and checks programs itself. Stage 0 is an oracle during development, not a runtime dependency. |
| D2 | **Integers.** One primitive `U32`: unboxed `i32`, wrapping `+ - *`, total `/ %` (zero divisor yields `0`), low-five-bit shifts, comparisons to `Bool`. `Nat` remains for proofs and indices; no automatic conversion. |
| D3 | **Text and bytes.** Primitive `String` (immutable UTF-8 with O(1) length and slicing) and `Bytes` (immutable byte sequence). Output is built as a chunk list plus one `concat`. No mutation and therefore no ownership design. |
| D4 | **Collections.** `Option`, `Result`, and `Ordering` are prelude enums. `Map[K, V]` is written in Neo as a size-balanced tree; it is not a primitive. |
| D5 | **I/O.** `argv`, `stdin.read_all`, `file.read`, `file.write`, `stderr.write`, all total and returning `Result[T, IoError]`. `main` returns `U32` as the exit code. The WIT world grows to `wasi:cli/environment`, `wasi:cli/stderr`, `wasi:cli/exit`, `wasi:filesystem/{preopens,types}`, `wasi:io/streams`. |
| D6 | **Loops.** No new surface form. Structural recursion only; WASM `return_call` is emitted for tail-position self calls. The compiler is written accumulator-style. |
| D7 | **Memory.** Per-phase arenas plus size-class free lists in the generated runtime. No GC or reference counting in v1; data is acyclic. |
| D8 | **Kernel totality.** `eval` gets a deterministic fuel budget (constant plus term-size allowance) and reports `normalization budget exceeded`. Where Stage 0 diverges, the harness records a rejection class. |
| D9 | **Modules.** `module M { ... }`, qualified `M.f(x)`, and `use M` at the top of a file. Single-file compilation; no package manager. |
| D10 | **Component emission.** A fixed-world emitter: the component shell is static bytes and only the core module varies. Stage 0 is migrated to this canonical form and frozen. General WIT parsing is not ported. |
| D11 | **Dev tooling.** A separate `neo-internal` binary exposes elaborate/check/report for the test suite. The public CLI stays `neo`, `neo FILE`, `neo build FILE`; there is no `neo check`. |
| D12 | **Parity strictness.** Acceptance, inferred types, and runtime output must match exactly. Diagnostic wording need not. |
| D13 | **Performance budget.** Stage 1 self-compiles in under ~60 s and under ~1 GiB under `wasmtime`. Measured at M5 and M9; adjusted only with a recorded reason. |
| D14 | **Determinism.** No `HashMap` iteration in emission, all name tables sorted, no clock or environment input, and CI double-builds each corpus program. |
| D15 | **Bindings.** `let name = e` and `let name: T = e` are immutable block bindings. The earlier no-`let` rule is relaxed. A binding annotation is parsed at comparison precedence, so function and equality types in annotations are parenthesized. |
| D16 | **Equality split.** `=` is the propositional equality type and the `let` separator. `==` and `!=` are boolean equality over `Nat`, `U32`, `Bool`, `String`, and `Unit`. |
| D17 | **Directed numerals.** A numeral has no default type. The expected type selects `Nat` or `U32`; the `u32` suffix pins `U32`; inference without an expected type is rejected. `(e : T)` ascription is available. `Nat` literals above 1,000,000 are rejected. |
| D18 | **Infix operators.** `* / %`, `+ -`, `<< >>`, `&`, `^`, `\|`, `< <= > >=`, `== !=`, and `=` with the documented precedence. Arithmetic, bitwise, shift, and comparisons are overloaded over `Nat` and `U32`; `Nat` implementations are ordinary total prelude functions, `U32` implementations are the existing primitives. |
| D19 | **No `USize`.** A bounded inductive machine type with proof-carrying `succ` and arithmetic was considered and rejected as too costly for the current milestone. `Nat` remains the inductive type for proofs and indices; `U32` remains the 32-bit machine type for computation. |
| D20 | **Text and byte layout.** `String` and `Bytes` are primitives backed by a 12-byte descriptor `{data, start, len}`; a value is a single `i32` pointer. Compiled code has O(1) length and slicing (slicing allocates only a descriptor), and concatenation allocates a fresh buffer. Descriptors and buffers are bump-allocated and never freed. M5 revisits allocation. The interpreter models slices by copying. |
| D21 | **Text indices are `U32`.** `str_length`, `bytes_length`, `bytes_get`, and both slices use `U32`; `Nat` stays for proofs. |
| D22 | **Byte literals.** `b"..."` holds ASCII plus `\xNN`. String literals are UTF-8 and reject `\xNN` above `7f`; both backends print `Bytes` as `b"\xNN..."`. |
| D23 | **String validity.** `String` is expected to contain UTF-8, but the runtime does not validate it and slicing is byte-based, so callers slice on character boundaries. `bytes_to_str` waits for `Result` at M4. |
| D24 | **Literal interning.** Each distinct literal bytes sequence is stored once in the data segment with a static descriptor. Lookup is deterministic and does not iterate a hash map. |

## 3. The `Neo-B` subset

The compiler is written in a subset of the language it accepts. The rule is
mechanical: **everything Stage 1 uses must already work in Stage 0**, enforced
by the Stage 1 build itself.

| Tier | Contents | Introduced by |
| --- | --- | --- |
| B0 | enums, `fn`, `match` with nested patterns, implicit `[]` parameters, unary `Nat` | exists |
| B1 | `U32`, primitive `String`/`Bytes`, `List`, `Option`, `Result`, `Ordering`, `Map`, `Bool` | M2, M3, M7 |
| B2 | first-class functions and closures, indexed enums, `use`/modules, file and argv I/O | M6–M8 |
| B3 | everything the elaborator needs: implicit inference, block and `match` type preservation, universe levels | M10 |

Stage 1 deliberately avoids: tactics (there are none), `Exists`-heavy
reasoning, dependent `Vec` in its own data structures, and the REPL.

Out of scope for v1: general WIT worlds, tactics, parallelism, garbage
collection, a package manager, IDE support, and a Neo-written REPL.

## 4. Milestones

Each milestone ends with: `cargo fmt --check`, clippy native and
`wasm32-wasip2`, the full test suite, all examples, and a determinism check.
Parity against Stage 0 is added by M1 and applies from then on.

| M | Deliverable | Gate | Size |
| --- | --- | --- | --- |
| M0 | This document and the decision record | Accepted | — |
| M1 | `neo-internal` oracle and corpus manifest (verdict, type, build hash) | Runs on all examples; catches a seeded regression; double-build stable | ~600 Rust |
| M2 | `U32`: kernel terms, literals, unboxed codegen, tests, docs | Corpus green, deterministic | ~800 Rust |
| M2.5 | Surface syntax: `let`, `=`/`==`, directed numerals, infix operators, ascription | Corpus green; Nat and U32 operator tests | ~700 Rust + ~400 Neo |
| M3 | Primitive `String`/`Bytes` and literal lowering | Corpus green; prelude migrated | ~900 Rust |
| M4 | I/O and world growth, `Result`, `main(): U32` | File-reading program runs under `wasmtime` | ~700 Rust |
| M5 | Arenas, free lists, `return_call`, benchmark | Self-compile-shaped workload in budget | ~700 Rust |
| M6 | Modules | Multi-module example builds and runs | ~600 Rust |
| M7 | Neo-written standard library (`Option`, `Result`, `Ordering`, `Map`, string utilities) | Library tests pass | ~1,500 Neo |
| M8 | Canonical component emitter, frozen | Byte-identical to `wit-component` on the corpus, stable across processes | ~400 Rust |
| M9 | Kernel port: core, eval + fuel, conversion, checking, termination, positivity, pretty | Verdict and type parity on the soundness suite and adversarial cases | ~3,500 Neo |
| M10 | Frontend port: lexer, parser, surface, elaborator | Parse/elaborate/type-dump parity, including implicit inference | ~5,000 Neo |
| M11 | Codegen port | Byte-identical core modules against Stage 0 on the corpus | ~3,500 Neo |
| M12 | Self-compilation: Stage 1 → Stage 2 → Stage 3 | Fixpoint, full differential, performance, CI | — |

## 5. Risks

1. **Elaborator heuristics** (`crates/neo-frontend/src/elab.rs`) are the least
   specified code in the repository. Mitigation: M0/M1 capture inferred types
   as goldens before the port; anything that cannot be specified is refactored
   in Stage 0 first.
2. **Normalization performance.** Pure, allocation-heavy NBE over a ~20k-line
   program. Mitigation: D8 fuel plus the M5 benchmark.
3. **Component byte-identity.** `wit-component` makes naming and ordering
   choices. Mitigation: D10 makes Neo's emitter the definition of canonical.
4. **Stack depth.** Structural recursion over list length reaches 20k+ frames.
   Mitigation: D6 tail calls and accumulator style.
5. **Memory.** Bump-only allocation cannot survive elaboration. Mitigation: D7
   arenas, measured at M5.
6. **Fuel divergence.** Stage 0 may not terminate where Stage 1 rejects.
   Mitigation: D8 classification and a corpus of known-divergent cases.
7. **Scope creep.** Full language vs `Neo-B`, general WIT, exact diagnostics.
   Mitigation: the decisions above are fixed.

## 6. Oracle protocol

`neo-internal` is a workspace binary used only by tests and development. It
reports, per program, in a deterministic text form:

- the verdict (accepted, or the error class);
- every declaration's type, rendered by the kernel pretty printer;
- the built component's byte length and a stable 64-bit hash of its bytes.

`tests/corpus.txt` stores that report for every file in `examples/` and
`tests/corpus/`. A change in the report is a deliberate diff, not an accident.
The test process is separate from the process that generated the file, so any
dependence on `HashMap` iteration order or other process-local state shows up
as a failure.

Runtime parity is checked separately: corpus programs are built and run under
`wasmtime`, with expected stdout, when the runtime is available.
