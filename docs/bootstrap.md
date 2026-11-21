# Self-hosting Neo

Neo is currently **Stage 0**: a Rust interpreter and WASI 0.2 backend. The goal
is a compiler written in Neo that compiles itself, with a byte-identical
fixpoint at Stage 3.

This document records the scope, the decisions that shape it, and the order of
work. Milestones **M0–M4a** are implemented: the decision record, the
deterministic corpus oracle, the primitive `U32` layer, the surface-syntax
revision, primitive `String`/`Bytes`, receiver-keyed methods, and
`Option`/`Result` with checked UTF-8 conversion. The first I/O step, **M4b.1**,
adds the `IO` modality and the `main(): IO[T]` entry point. **M4b.2** adds
`extern` host capabilities and `stderr::write`.

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
| D5 | **I/O is an `IO` modality.** Effectful operations have type `IO[T]`, not `Result[T, IoError]`: `argv`, `stdin.read_all`, `file.read`, `file.write`, and `stderr::write` are `IO` actions that convert host errors inside the action, and `main` has type `IO[U32]`. Its value is printed; a `U32` result is also the process exit status. The WIT world grows to `wasi:cli/environment`, `wasi:cli/stderr`, `wasi:cli/exit`, `wasi:filesystem/{preopens,types}`, `wasi:io/streams`. |
| D6 | **Loops.** No new surface form. Structural recursion only; WASM `return_call` is emitted for tail-position self calls. The compiler is written accumulator-style. |
| D7 | **Memory.** Per-phase arenas plus size-class free lists in the generated runtime. No GC or reference counting in v1; data is acyclic. |
| D8 | **Kernel totality.** `eval` gets a deterministic fuel budget (constant plus term-size allowance) and reports `normalization budget exceeded`. Where Stage 0 diverges, the harness records a rejection class. |
| D9 | **Modules.** `mod M { ... }` groups items, `mod M;` loads `M.neo` or `M/mod.neo`, `M::f(x)` reaches one by path, and `use` imports items, aliases, or constructor globs. No package manager. |
| D10 | **Component emission.** A fixed-world emitter: the component shell is static bytes and only the core module varies. Stage 0 is migrated to this canonical form and frozen. General WIT parsing is not ported. |
| D11 | **Dev tooling.** A separate `neo-internal` binary exposes elaborate/check/report for the test suite. The public CLI stays `neo`, `neo FILE`, `neo build FILE`; there is no `neo check`. |
| D12 | **Parity strictness.** Acceptance, inferred types, and runtime output must match exactly. Diagnostic wording need not. |
| D13 | **Performance budget.** Stage 1 self-compiles in under ~60 s and under ~1 GiB under `wasmtime`. Measured at M5 and M9; adjusted only with a recorded reason. |
| D14 | **Determinism.** No `HashMap` iteration in emission, all name tables sorted, no clock or environment input, and CI double-builds each corpus program. |
| D15 | **Bindings.** `let name = e` and `let name: T = e` are immutable block bindings. The earlier no-`let` rule is relaxed. A binding annotation is parsed at comparison precedence, so function and equality types in annotations are parenthesized. |
| D16 | **Equality split.** `=` is the propositional equality type and the `let` separator. `==` and `!=` are boolean equality over `Nat`, `U32`, `Bool`, `String`, and `Unit`. |
| D17 | **Directed numerals.** A numeral has no default type. The expected type selects `Nat` or `U32`; the `u32` suffix pins `U32`; inference without an expected type is rejected. `(e : T)` ascription is available. `Nat` literals above 1,000,000 are rejected. |
| D18 | **Infix operators.** `* / %`, `+ -`, `<< >>`, `&`, `^`, `\|`, `< <= > >=`, `== !=`, and `=` with the documented precedence. Arithmetic, bitwise, shift, and comparisons are overloaded over `Nat` and `U32`; `Nat` implementations are ordinary total prelude functions, `U32` implementations are the existing primitives. |
| D19 | **No `USize`.** A bounded inductive machine type with proof-carrying `Succ` and arithmetic was considered and rejected as too costly for the current milestone. `Nat` remains the inductive type for proofs and indices; `U32` remains the 32-bit machine type for computation. |
| D20 | **Text and byte layout.** `String` and `Bytes` are primitives backed by a 12-byte descriptor `{data, start, len}`; a value is a single `i32` pointer. Compiled code has O(1) length and slicing (slicing allocates only a descriptor), and concatenation allocates a fresh buffer. Descriptors and buffers are bump-allocated and never freed. M5 revisits allocation. The interpreter models slices by copying. |
| D21 | **Text indices are `U32`.** `str_length`, `bytes_length`, `bytes_get`, and both slices use `U32`; `Nat` stays for proofs. |
| D22 | **Byte literals.** `b"..."` holds ASCII plus `\xNN`. String literals are UTF-8 and reject `\xNN` above `7f`; both backends print `Bytes` as `b"\xNN..."`. |
| D23 | **String validity.** `String` is expected to contain UTF-8, but creating or slicing one does not validate it, and slicing is byte-based, so callers slice on character boundaries. `Bytes.to_str` is the checked conversion and reports invalid input. |
| D24 | **Literal interning.** Each distinct literal bytes sequence is stored once in the data segment with a static descriptor. Lookup is deterministic and does not iterate a hash map. |
| D25 | **Receiver-keyed methods.** A method is declared inside `impl Type { ... }` (or `impl[T: Type] Type[T] { ... }`) and stored as `Type.name`. The first explicit parameter is the receiver and must have the impl target's type. Calls `e.name(args)` dispatch statically on the receiver's type head; generic receivers unify their parameters. Methods may be declared on enums and primitive types, and the same simple name may exist on several types. Methods are private unless declared `pub`. |
| D26 | **No classes or method values.** Methods are not values: `xs.length` is invalid and a call must supply every explicit argument, including `()`. There is no unqualified overloading, no type classes or instances, and no user-defined operator overloading. `==`/`!=` remain fixed. |
| D27 | **Modules use `::`.** Qualified module access is `M::f(x)`; `.` is reserved for method calls. This removes the ambiguity between `M.f(x)` and a method on a value named `M`. |
| D28 | **Internal primitives.** The `u32_*` operations are the kernel/operator implementation and cannot be written in a program; using one reports the corresponding operator. The low-level `str_*`/`bytes_*` operations are likewise unspellable; the prelude `impl` methods are the only public surface. The prelude itself is elaborated in a trusted mode that may call them. |
| D29 | **`Option`, `Result`, and `Utf8Error` are prelude enums.** `Option[T]` has `None` and `Some(T)`; `Result[T, E]` has `Ok(T)` and `Err(E)`; `Utf8Error` has `Invalid(U32)`. They are ordinary inductives with no special typing or printing rules, so `main`'s action may produce any of them. |
| D30 | **Checked UTF-8.** `bytes_utf8_check` returns `0` for valid bytes and otherwise the 1-based index of the first byte of the first invalid sequence; `str_from_bytes` is the unchecked inverse of `str_to_bytes`. `Bytes.to_str` combines them into `Result[String, Utf8Error]`, whose error carries the 0-based start of the offending sequence. Validation is strict: overlong forms, surrogates, code points above `U+10FFFF`, and truncated sequences are rejected. |
| D31 | **Primitive results stay primitive.** A primitive operation returns only a primitive type, so structured results are assembled in Neo. `Bytes.to_str` is the model: a `U32` check code plus a prelude `match`. This also applies to `IO`: a primitive cannot return an action. Host functions that produce structured results are declared `extern` with an `IO` type and are introduced in M4b.2. |
| D32 | **`IO` is the effect design.** `IO[T]` is a type former with no constructors, no eliminator, and no surface `run`. The kernel provides `IO::pure : {A} -> A -> IO[A]` and `IO::bind : {A,B} -> IO[A] -> (A -> IO[B]) -> IO[B]`; neither has a computation rule, so actions are compared structurally. `extern` declarations are typed in `IO`, and only the program entry point interprets `main`'s action, once. This keeps every data type canonical: no effectful term is ever a stuck neutral at a data type, and no primitive smuggles an action through a status-code side channel. Effect rows were rejected as unnecessary, and a world-token model was rejected because Neo has no linearity. |
| D33 | **Closed host capabilities.** `mod ns { extern fn name(params): IO[T] = "import" }` declares a capability inside a module. Externs are not values: the kernel stores each declaration in `Globals::externs`, checks saturated calls against that type, and evaluates them to an opaque `Action::Extern`. Bare or partial use is rejected, so a well-typed program's actions are always executable. Import strings are validated against a closed registry shared by the elaborator, interpreter, and WASM backend (`neo-host`): unknown imports and mismatched declared types are rejected before either backend runs. The registry lowers `wasi:cli/stderr@0.2.9#write` to `get-stderr` plus a chunked `blocking-write-and-flush` loop. Host errors are returned as host-built `Result[Unit, IoError]` values, and a `stream-error` resource is dropped before reporting `Closed` or `Failed`. Extern parameters are restricted to primitives, and the emitter imports the full fixed capability set. |

| D34 | **Modules, visibility, and `impl`.** A module is `mod name { ... }`; modules and items form one namespace per module and may be nested. `use` takes root-relative paths with `self`/`super`, supports `as` aliases, `Type::{A, B}` groups, and `Type::*` constructor globs; `pub use` re-exports public items and modules. `mod name;` loads files through `load::parse_file`, searching `name.neo` then `name/mod.neo` with cycle and ambiguity checks. Items and methods are private by default and visible from their module and its descendants; `pub` is the only widening. `impl Type { ... }` and `impl[T: Type] Type[T] { ... }` add methods, and several impl blocks may target one type as long as method names do not collide. Items within a module are elaborated in dependency order. Prelude types and constructors are auto-imported, while other prelude items require `use`. Constructors are namespaced (`Nat::Succ`), the prelude uses CamelCase variants, and `IO::pure`/`IO::bind` are associated forms on the built-in `IO`. |

| D35 | **Process and file I/O.** `env::argv(): IO[List[String]]` lowers to `wasi:cli/environment#get-arguments`; `stdin::read_all(): IO[Result[Bytes, IoError]]` lowers to `get-stdin` plus blocking reads and treats `stream-error::closed` as end of stream; `file::read(path)` and `file::write(path, data)` lower to preopens plus `descriptor.open-at` and `descriptor.read`/`write`. File paths are relative to a preopened directory, granted with `wasmtime --dir .` in the tests. The emitter exports `cabi_realloc` for list-returning imports. `argv` is infallible; file and stdin failures become `IoError::Failed`. |

| D36 | **Exit status.** A `main` whose action produces `U32` prints the value and exits with its low eight bits; any other result type exits `0`. The interpreter and compiled component agree. The vendored `wasi:cli@0.2.9` copy marks `exit-with-code` as `@since(version = 0.2.0)` rather than `@unstable`, because the fixed world uses it directly; wasmtime provides it without a feature flag. |

| D37 | **I/O ergonomics.** `IoError` distinguishes `Closed`, `Failed`, `NotFound`, `PermissionDenied`, and `InvalidInput`; WASI error codes are mapped to the closest variant. `stdout::write` mirrors `stderr::write`, `env::args` omits argv0 while `env::argv` keeps it, `env::get` returns `Option[String]`, and `dir::create`/`dir::list` cover directory creation and enumeration. Directory listings are returned in host order. |

| D38 | **Direct calls, tail calls, and allocation.** A saturated application of a known top-level function calls an uncurried direct entry point instead of allocating and applying a curried closure. Tail-position self calls emit `return_call`. `neo-internal bench` measures component size and wasmtime wall time on `bench/list_workload.neo`. Free lists are deferred: Neo has no liveness signal, and the measured removable cost was call-site closure allocation; a generic free list needs an ownership model. |

| D39 | **Fixed component shell.** The component wrapper is a committed binary shell produced once from the frozen world. Codegen parses the top-level component sections, replaces the first core-module section with the generated module, and leaves the shim modules and wrapper untouched. Runtime WIT parsing and `wit-component` are no longer dependencies; the shell is regenerated only when the world changes. |

| D40 | **Source-attributed spans.** `Span` carries a `file` id, and `Program` carries the loader's file table. The loader assigns ids while parsing each module file, so diagnostics from child files print the correct path. In-memory `parse_source` uses id 0 and no file table. |
| D41 | **Deep terms.** The native CLI and `neo-internal` run on a 64 MiB worker thread, and the `wasm32-wasip2` compiler links with a 16 MiB stack. This removes the observed aborts on large `Nat` literals without changing kernel semantics; an explicit recursion budget remains future work. |

| D42 | **Stage 1 readiness spike.** `spike/` validates fuel-driven evaluation, function-valued closure environments, byte parsing, and a pure wasm byte builder. Nested `List[Val]` environments are rejected by positivity, so closures use `fn(String) -> Val`; such environments cannot be compared, so conversion must apply closures to fresh variables. Mutual recursion is still absent; the ports use single-entry recursive functions or need a future mutual-recursion feature. |

| D43 | **Stage 1 kernel slice.** `stage1/kernel` ports terms, values, fuel-driven evaluation, quoting, conversion, sorts, `infer`/`check` for variables, universes, Pi, lambda, and application, a global environment for enum and constructor typing, non-dependent match checking with constructor reduction, a first `Eq`/`ReflTm`/`J` slice, a conservative positivity check, a pretty printer for the current term subset, and a conservative termination check. Argument lists are encoded with `TNil`/`TCons` and `VNil`/`VCons` because nested inductive fields are rejected. Environments are `fn(U32) -> Val`; Pi/lambda closures store bodies and environments, and conversion applies them to fresh variables. Global-environment typing for constructors and matches, positivity, termination, and printing remain. |

| D44 | **Stage 1 frontend/codegen/driver slices.** `stage1/frontend` lexes and parses a small arithmetic language with single-entry fuel-recursive state machines. `stage1/codegen` emits LEB128, sections, and a valid minimal wasm module using `U32.to_byte`, a new public primitive method (`bytes_from_u32` internally). `stage1/driver.neo` reads a source path and output path from `env::args`, parses the file through the Stage 1 frontend, emits a wasm module returning the parsed value, writes it, and runs as a WASI component. Full syntax, elaborator, and component emission remain. |

## 3. The `Neo-B` subset

The compiler is written in a subset of the language it accepts. The rule is
mechanical: **everything Stage 1 uses must already work in Stage 0**, enforced
by the Stage 1 build itself.

| Tier | Contents | Introduced by |
| --- | --- | --- |
| B0 | enums, `fn`, `match` with nested patterns, implicit `[]` parameters, unary `Nat` | exists |
| B1 | `U32`, primitive `String`/`Bytes`, `List`, `Option`, `Result`, `Ordering`, `Map`, `Bool` | M2, M3, M7 |
| B2 | first-class functions and closures, indexed enums, `use`/modules, file and argv I/O | M4b, M6 |
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
| M3.5 | Receiver-keyed methods; `u32_*` retired from the surface | Method tests green; corpus green | ~500 Rust + ~150 Neo |
| M4a | `Option`, `Result`, `Utf8Error`; checked `Bytes.to_str` | Valid and invalid inputs agree in both backends | ~250 Rust + ~60 Neo |
| M4b.1 | `IO[T]`, `IO::pure`/`IO::bind`, `main(): IO[T]` | `fn main(): IO[U32] { IO::pure(0) }` agrees in both backends | ~250 Rust |
| M4b.2 | `extern fn ... = "wasi:...#name"`, explicit imports, host-built results (`stderr::write`) | External write runs under `wasmtime`; interpreter parity where possible | ~200 Rust |
| M4b.3 | World growth: `argv`, `stdin.read_all`, `file.read`, `file.write` | File-reading program runs under `wasmtime` | ~150 Rust |
| M4b.4 | Exit status through `wasi:cli/exit#exit-with-code` | Compiled exit status matches `main`'s `U32` | ~100 Rust + spike |
| M5 | Direct calls, `return_call`, allocation review, benchmark | Self-compile-shaped workload in budget | ~700 Rust |
| M6 | Modules, file modules, `pub use`, visibility, and `impl` blocks | Multi-file module builds and runs; module, visibility, and re-export tests | ~700 Rust |
| M7 | Neo-written standard library (`Option`, `Result`, `Ordering`, `Map`, string utilities) | Library tests pass | ~1,500 Neo |
| M8 | Canonical component emitter, frozen | Stable components across processes; fixed shell with core-module splicing | ~400 Rust |
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
