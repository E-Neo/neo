# Stage 1 readiness spike

This directory contains small Neo programs used to test assumptions for the
self-hosting ports.

## What works

- Fuel-driven evaluation with explicit `Nat` fuel. Every recursive call
  decreases the fuel structurally, so the termination checker accepts it.
- Closures whose environment is a **function** `fn(String) -> Val`.
  `Val` may contain that function without violating positivity.
- Byte-oriented parsing using `Bytes.get`, `U32` arithmetic, and fuel for the
  scan position.
- A pure chunk-tree `Bytes` builder and a valid minimal wasm module.
- The whole spike compiles to a WASI component and runs under wasmtime.

## Findings

- A closure environment stored as `List[Val]` inside `Val` is rejected:
  nested inductives are unsupported. A function-valued environment works, and
  compiles, but environments cannot be compared for equality.
- Mutual recursion is not needed for these slices, but a full lexer/parser or
  kernel needs either single-entry state machines or a mutual-recursion
  feature.
- The kernel's `Val` cannot be structurally compared if closures carry
  function environments. Conversion must compare closures by applying them to
  fresh variables, not by comparing their environments.
- Fuel is the practical termination mechanism for evaluators; the kernel port
  should use an explicit fuel parameter and a documented budget.
- `U32` positions and `Nat` fuel create conversion friction; the standard
  library needs `Nat`/`U32` conversion helpers.
- `Bytes` rendering is a recursive concat. It is fine for small outputs; the
  codegen port needs a benchmark with a multi-megabyte module.
