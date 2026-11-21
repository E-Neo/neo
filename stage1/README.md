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
- A conservative positivity check over enum constructor telescopes. Direct
  positive occurrences are accepted; negative occurrences in function
  domains and nested occurrences under another type constructor are rejected.
- A pretty printer for the current term subset, including U32 decimal
  formatting, de Bruijn names, Pi/lambda/application printing, and equality.
- A conservative termination check. Function bodies are traversed with a set
  of variables known to be strict subterms; match branch fields are added to
  that set, and every recursive call must pass one of them (or a constructor
  built from one).
- Alpha-renamed lambdas, different universes, unbound variables, fuel
  exhaustion, identity checking, a type error, Pi inference, and application
  inference are tested.

## Next

- Dependent match refinement and full motive application in `J`.
- A closer match to Stage 0's termination acceptance and rejection behavior.
- Differential acceptance against Stage 0 on the soundness and corpus suites.

## Frontend, codegen, and driver slices

`stage1/frontend/` contains a fuel-driven lexer and a shunting-yard
compiler for a small arithmetic language. It supports numerals, `+`, `*`,
and parentheses, and emits wasm instructions directly. The parser avoids
mutual recursion.

`stage1/codegen/` emits LEB128, wasm sections, and a valid module from
either a constant or an instruction sequence using the new `U32.to_byte`
method. `component::splice` replaces the first core-module section in the
frozen component shell. The shell offsets are constants and must be
regenerated if the shell changes.

`stage1/driver.neo` is a first end-to-end Stage 1 compiler: it reads a source
path and an output path from `env::args`, reads the source, parses the
arithmetic expression, emits a wasm module returning that value, and writes
it to the output path. It builds and runs as a WASI component under wasmtime;
the emitted module validates and returns the parsed value.
