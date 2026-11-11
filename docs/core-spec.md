# The Neo Core Specification

This document specifies the **trusted core** of Neo: the small calculus that the
kernel re-checks after elaboration. It is the reference for anyone auditing the
proof checker. It describes the rules the implementation actually enforces in
`src/check.rs`, `src/conv.rs`, `src/eval.rs`, `src/quote.rs`,
`src/positivity.rs`, and `src/recursion.rs`.

The surface language, the elaborator, the interpreter, and the WASM backend are
**not** part of the trusted base. They may be replaced or rewritten. The
guarantee is:

> If the kernel accepts a program, then every closed term of a type in that
> program is well-typed in the core calculus below, and reduction is total.

The kernel is the trust anchor because Neo is meant to prove facts about its own
compiler. A bug in the elaborator can at worst produce a term the kernel
rejects; it cannot manufacture a proof the kernel accepts unless it exploits a
bug in the rules written here.

## 1. Terms

Core terms use de Bruijn indices. Variable `0` is the innermost binder.

```text
t, A, B ::=
  | i                       variable (de Bruijn index)
  | c                       global function constant
  | Type u                  universe, u : Nat
  | Prop                    impredicative sort
  | fn(x: A) -> B           dependent function type
  | fn(x: A) => t           lambda
  | t(u)                    application
  | E(args)                 enum type former
  | C(args)                 constructor value
  | match m, s { arms }     enum elimination
  | Eq(A, x, y)             equality type
  | refl(x)                 reflexivity proof
  | J(A, x, y, p, m, r)     equality elimination
```

The core has **no let-binding**. Surface blocks and variable patterns elaborate
to `(fn(x: A) => body)(value)`.

Branches in a `match` are records:

```text
{ ctor: C, binders: n, body: t, impossible: b }
```

`binders` is the number of fields bound by the constructor pattern. The branch
body is checked in the context extended with those fields. Branches are stored
in constructor-declaration order; branch `i` belongs to constructor `i`.

## 2. Contexts and environments

A typing context `Gamma` is an ordered list of entries `(x, A)`. The level of a
context is its length. A variable at level `i` denotes the `i`-th entry, and its
de Bruijn index relative to the end of the context is `level - 1 - i`.

An environment maps a context to values. Evaluation is call-by-name with
sharing through `Rc`. `eval(t, rho)` is defined by structural recursion on `t`
and weak-head reduction of the resulting value. `force` unfolds neutral
computations as needed.

## 3. Values and normalisation by evaluation

```text
v ::=
  | Type u
  | Prop
  | (x: A) -> B            Pi value
  | fn(x: A) => t          closure
  | E(v*)                  enum type
  | C(v*)                  constructor
  | Eq(A, x, y)
  | refl(x)
  | global c               unapplied function constant
  | neutral
```

Neutrals are stuck variables, globals, applications, matches, and `J`
eliminations. Quote (`quote`) converts a value back to a term at a given level.
All evaluation of well-typed, terminating terms is total; the kernel never
executes a program that has not passed the termination check.

## 4. Judgments

The kernel implements two mutually recursive judgments:

```text
Gamma |- t : A       infer
Gamma |- t <= A      check
```

- `infer` computes a type for `t`.
- `check` verifies `t` against a known expected type. Lambdas and `refl` are
  checked structurally; every other term is inferred and then compared with
  `conv_ty`.

Type formation uses:

```text
Gamma |- A : Sort
```

where `Sort ::= Prop | Type u`.

### 4.1 Variables and globals

```text
Gamma(x) = A
----------------
Gamma |- x : A

globals(c) = A
----------------
Gamma |- c : A
```

Global function types and bodies are registered before checking. A body is only
unfolded by conversion after it has passed both the termination check and the
kernel's own type check. Before that it is opaque.

### 4.2 Universes

```text
Gamma |- Type u : Type (u + 1)

Gamma |- Prop : Type 0
```

`Prop` is impredicative: if the codomain of a function type is `Prop`, the
whole function type is `Prop` regardless of its domain. `Type` is predicated
and cumulative for type compatibility:

```text
Gamma |- A : Type u       Gamma, x: A |- B : Type v
--------------------------------------------------
Gamma |- (x: A) -> B : Type (max u v)

Gamma, x: A |- B : Prop
-----------------------
Gamma |- (x: A) -> B : Prop
```

Definitional equality is **exact**: `Type u = Type v` only when `u = v`.
Cumulativity (`Type u <= Type v` for `u <= v`) is used only when checking that
one type is usable where another is expected (`conv_ty`), never for equality
inside a type index.

### 4.3 Functions

```text
Gamma |- A : Sort       Gamma, x: A |- t : B
--------------------------------------------
Gamma |- fn(x: A) => t : (x: A) -> B

Gamma |- f : (x: A) -> B       Gamma |- a <= A
------------------------------------------------
Gamma |- f(a) : B[a/x]
```

`check` for a lambda verifies that its domain is convertible to the expected
domain and then checks the body against the instantiated codomain.

### 4.4 Enums

An enum declaration fixes:

```text
enum E(x1: A1, ..., xk: Ak) : Sort {
  C1(fields...) : E(indices...)
  ...
}
```

- Parameters `x1 ... xk` are the first arguments of every occurrence of `E`.
- A constructor's type is a telescope of the parameters, then its fields, ending
  in `E(indices...)`.
- Every constructor must return `E`.
- The number of result indices must equal the number of parameters.

Formation checks each argument against the corresponding parameter type, with
earlier arguments available to later ones:

```text
Gamma |- a1 <= A1
Gamma |- a2 <= A2[a1/x1]
...
Gamma |- E(a1, ..., ak) : Sort
```

Constructor application checks the full telescope (parameters and fields):

```text
Gamma |- C(a1, ..., an) : E(indices...)
```

The kernel computes the type by walking the stored constructor telescope; it
does not trust an index supplied by the elaborator.

## 5. Positivity and enum well-formedness

The kernel re-validates every enum from its stored telescope. These checks are
authoritative, not cosmetic:

1. **Strict positivity.** A constructor field may contain `E` only as a direct
   argument of the field type, not to the left of an arrow, inside an
   application head, or nested inside another inductive. Nested inductives are
   rejected.
2. **Result indices.** A constructor result index may not mention `E` at all.
   This is conservative: an occurrence in an index could otherwise hide an
   unsafe occurrence.
3. **Universe bounds.** For an enum sorted in `Type u`, every field type and
   every result index must have sort `<= u`. A `Prop`-sorted enum may have
   large fields, but may only be eliminated according to the singleton rule.
4. **Singleton elimination.** A `Prop` enum is eliminable into `Type` only if it
   is empty or has exactly one constructor and every field has sort `Prop`.
   Otherwise matches on it must return `Prop`.

If any of these checks fails, the program is rejected with a `kernel rejected
enum ...` error.

## 6. Equality

```text
Gamma |- A : Sort       Gamma |- x <= A       Gamma |- y <= A
-------------------------------------------------------------
Gamma |- Eq(A, x, y) : Prop

Gamma |- x <= A
----------------------------
Gamma |- refl(x) : Eq(A, x, x)
```

`Eq` is a propositional equality. It is not built in as a rewrite rule. Surface
`match` on an equality proof elaborates to `J`.

### 6.1 J

```text
Gamma |- A : Sort
Gamma |- x <= A
Gamma |- y <= A
Gamma |- p : Eq(A, x, y)
Gamma |- m : (z: A) -> Sort
Gamma |- r : m(x)
---------------------------------------------------------
Gamma |- J(A, x, y, p, m, r) : m(y)
```

The kernel checks the motive is a function whose domain is `A`, checks the
`refl` case at `x`, and returns the motive applied to `y`. The motive is applied
by evaluation; there is no textual replacement of `x` by `y`. This makes `J`
capture-avoiding even under binders.

Evaluation of `J` with a `refl` proof returns the `refl` case.

## 7. Match

A source match elaborates to a core `match` with an explicit **motive**: a
function from the scrutinee type to the result type. This is why matches can
depend on the scrutinee.

### 7.1 Motive

```text
Gamma |- s : E(params)
Gamma, z: E(params) |- m : Sort
```

The motive's domain must be convertible to the scrutinee type. When the expected
result type is known, the elaborate step abstracts the scrutinee term out of it;
when it is not known, the result type is inferred from the branches. The kernel
only sees the resulting motive and re-checks its domain.

### 7.2 Branch checking and index refinement

For each constructor `Ci` of `E`, let

```text
Ci : (params) -> (fields) -> E(indices)
```

Given a scrutinee type `E(s*)`, the kernel first unifies each `index` against the
corresponding `s*`. Refinement is deliberately first-order:

- The flexible variables are the surrounding context variables whose types can
  occur in an index (everything except `Prop`-typed evidence and equality
  proofs) together with the constructor's fields.
- A flexible variable may be solved by any index term, including one that
  mentions constructor fields. For example, matching `Vec(A, n)` against
  `vcons` solves the context index `n := succ(m)`.
- A constructor clash (`zero` against `succ(n)`, two different constructors, or
  two different enum names) means the branch is **impossible**.
- Any equation that is neither already definitionally equal nor a solvable
  flexible variable is **stuck**, and the match is rejected with an error.

Solutions are then flattened: a solution may mention another solved variable,
so chains are resolved until every value is expressed in unsolved variables
only. A cycle, or an equation that would solve a variable with a term containing
itself, is **stuck**; the branch is refused rather than guessed. This is the
occurs check.

Every solution must also inhabit the variable's declared type in the extended
context, after applying the other solutions. This is the soundness gate: a
branch may assume the equation only because it holds for every value the branch
can match. Finally, the refined constructor result must be convertible to the
refined scrutinee type.

For a possible branch, fields are bound in telescope order. Let `theta` be the
solutions for solved context variables and fields.

1. Push each field with its refined type `A[theta]`.
2. Form `Ci(params, fields)` where a solved field uses its solution and an
   unsolved field uses the freshly bound variable.
3. Check the branch body against `m(Ci(...))[theta]` in a context where every
   local type has `theta` applied and the evaluation environment maps each
   solved variable to its solution.

The branch body keeps the same de Bruijn layout. Only the stored local types and
the evaluation environment are refined, so a variable can be known to equal a
constructor field without rewriting the term.

The elaborator performs the same refinement (`check::refine_ctor`) so that the
surface branch is elaborated in the refined context. It additionally replaces
every solved branch-field variable in the body by its solution.

An impossible branch has no reachable value. The kernel does **not** check its
body. The branch is stored with `impossible: true`, and evaluation and code
generation never select it. A user may omit the arm entirely; if an arm is
written, its body is ignored. Writing an arm for a *reachable* constructor is
mandatory.

### 7.3 Exhaustiveness

`match` must contain a branch for every constructor whose result indices can
unify with the scrutinee type. Exactly one branch per constructor is stored, in
constructor order. If a constructor is reachable and its arm is missing, the
match is rejected as non-exhaustive.

Refinement is still first-order and conservative. A context index is refined
only when the equations determine it: matching `Vec(A, n)` works because the
branches force `n := zero` and `n := succ(m)`. An index whose head is stuck, for
example `Vec(A, pred(n))` where `pred` is an ordinary recursive function, cannot
be related to `zero` or `succ(...)`, so the match is rejected. `Prop`-typed
evidence is never solved, because it cannot occur in an index and solving it
would amount to proof search.

Surface nested patterns and several arms for one constructor are elaborated
into nested core matches. Each nested match has its own motive and is checked by
the rules above; the core calculus is unchanged.

## 8. Termination

All functions are total. A definition passes the termination checker only when:

1. every occurrence of the recursive name is the head of a fully applied spine
   of exactly its arity arguments; and
2. every recursive call is structurally decreasing in one fixed parameter
   position.

"Structurally decreasing" means the corresponding argument is a strict
subterm of the original parameter, as tracked through constructor-field
binders. An occurrence of the recursive name as a value, in a lambda, or under
a non-head application is rejected. This rules out recursion through escaped
closures.

There is no mutual recursion and no well-founded recursion in Stage 0.

## 9. Conversion

`conv` is definitional equality on forced values. It is exact:

- `Type u = Type v` iff `u = v`; `Prop = Prop`.
- Pi types compare domains and codomains under a fresh variable.
- Lambdas compare extensionally.
- Enums and constructors compare name, tag, and arguments exactly.
- Equality types compare all three components.
- Neutrals compare structurally.
- A global body may unfold only after it has been sealed as checked.

`conv_ty` additionally allows `Type u <= Type v` for `u <= v` and keeps Pi
domains exact while allowing cumulative codomains. Cumulativity never weakens
the equality used for indices, so it cannot be used to manufacture an index
equation.

## 10. Evaluation and erasure

Evaluation is normalisation by evaluation. `quote` is its inverse at a given
level. A `match` reduces only when the scrutinee is a constructor; otherwise it
is a neutral. An impossible branch is never selected. `J` reduces only when its
proof is `refl`.

Type-level terms, proofs, and equality witnesses are erased by the backend.
Runtime values are heap-allocated constructor blocks and closures. Erasure is
not trusted for typing: the kernel checks the full dependent term before any
erasure happens.

## 11. Trust boundary

Trusted (must be audited):

- the core term representation and de Bruijn operations;
- `eval`, `force`, and `quote`;
- `conv` and `conv_ty`;
- the type checker and match refinement in `check.rs`;
- positivity and enum re-validation;
- the structural termination checker.

Untrusted (bugs cause rejection or wrong code generation, not a false proof):

- lexer, parser, surface AST;
- elaborator, argument elision, index inference, and surface `J` construction;
- pretty printer and REPL;
- WASM code generation;
- the CLI.

The untrusted elaborator is still expected to emit core terms the kernel
accepts. The kernel re-checks every committed function and every enum from the
stored data, so a mismatch is detected rather than trusted.

## 12. Known limits

These are deliberate Stage 0 limits, not soundness holes:

- Index refinement is first-order: a stuck or compound scrutinee index is
  rejected rather than solved by proof search (see section 7.3).
- No mutual or nested inductives, no higher inductive types.
- No proof irrelevance in conversion. Two different proofs of the same
  proposition are not automatically convertible.
- No general well-founded recursion; only structural recursion.
- No universe polymorphism: `Type u` levels are concrete.
- Large elimination from `Prop` is only allowed under singleton elimination.
- `Prop -> Prop` families are conservatively sorted in `Type 0`.

Future compiler-correctness work should target a proof-producing translation to
a small core IR, with the kernel above as the trust anchor. Proving the WASM
backend correct is a separate, larger obligation.
