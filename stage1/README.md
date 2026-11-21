# Stage 1 kernel slice

This is the first ported kernel slice: core terms, values, evaluation, quoting,
and conversion, written in Neo.

## Representation

- `Tm` and `Val` encode argument lists with `TNil`/`TCons` and
  `VNil`/`VCons`. A `List[Tm]` or `List[Val]` field would be a nested inductive
  and is rejected by the positivity checker.
- Environments are functions `fn(U32) -> Val`, using de Bruijn indices.
- Evaluation and quoting take explicit `Nat` fuel; every recursive call
  decreases it.
- Pi and lambda closures store their body plus the environment, not a
  function over values. Conversion applies them to fresh `VVars`.

## Covered

- `Var`, `Const`, `Ty`, `PropTy`, `Ind`, `Ctor`, `Pi`, `Lam`, and `App`.
- TList/TNil list nodes.
- `eval`, `quote`, `normalize`, and `conv`.
- Sorts and `infer`/`check` for variables, universes, Pi, lambda, and
  application, with a `List[Val]` context and context-environment functions.
- A Stage 1 global environment (`EnumInfo`/`CtorInfo`/`CtorList`) and typing
  for enum values and constructor applications via `apply_type`.
- A `Match` term, encoded branch lists, non-dependent branch checking with
  constructor field telescopes, exhaustiveness checks, and constructor
  reduction during evaluation.
- `Eq`, `ReflTm`, and a first `J` slice. Equality values are encoded as
  `VInd("Eq", ...)` and proofs as `VCtor("Refl", ...)`, so the existing
  conversion code handles them. `Refl` checks both sides against the value;
  `J` checks the proof and reduces to its `refl_case`. Motives are not yet
  applied.
- Alpha-renamed lambdas, different universes, unbound variables, fuel
  exhaustion, identity checking, a type error, Pi inference, and application
  inference are tested.

## Next

- Dependent match refinement and full motive application in `J`.
- Positivity, termination, and pretty printing.
- Differential acceptance against Stage 0 on the soundness and corpus suites.

## Frontend, codegen, and driver slices

`stage1/frontend/` contains a fuel-driven lexer and parser for a small
arithmetic language. The lexer uses a byte-class state machine; the parser
sums numerals with a single recursive token walk. Both avoid mutual recursion.

`stage1/codegen/` emits LEB128, wasm sections, and a valid minimal module
using the new `U32.to_byte` method.

`stage1/driver.neo` is a first Stage 1 entry point: it reads a path from
`env::args`, reads the file through `file::read`, runs the Stage 1 parser, and
returns `Ok(value)` or `Err(InvalidInput)`. It builds and runs as a WASI
component under wasmtime.
