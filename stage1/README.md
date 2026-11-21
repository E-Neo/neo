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
- Alpha-renamed lambdas, different universes, unbound variables, and fuel
  exhaustion are tested.

## Next

- Checking/inference: sorts, Pi domains/codomains, constructors, match, and J.
- Positivity, termination, and pretty printing.
- Differential acceptance against Stage 0 on the soundness and corpus suites.
