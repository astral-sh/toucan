# Vector shuffles

`__builtin_shufflevector` supports fixed vectors in all seven compiler profiles.
The first two value operands are each evaluated once, without a promised relative
order. Scalar index expressions are checked as integer constant expressions and
do not execute at runtime. Indices select from the concatenated input vectors;
signed `-1` permits an undefined output lane. Unsigned all-ones values are not
that sentinel, and other negative or out-of-range indices are rejected.

Clang requires matching input vector types. GNU allows different input lengths
when the element types match. GNU produces a canonical generic vector type;
Clang preserves the first input's vector identity and typedef alignment when the
result lane count is unchanged. Changing the lane count produces a generic type.

Clang also accepts a two-argument form. Its second argument is an integer mask
vector with the first argument's lane count; the mask element width may differ.
Each dynamic mask value is reduced modulo the input lane count. Negative mask
values select lanes too: dynamic `-1` selects the last input lane rather than an
undefined value. Both value operands still execute once.

## Retained semantics

`ExprKind::ShuffleVector` owns the two `ExprUse` edges, the written builtin callee
occurrence, and a `ShuffleMask`. A constant mask stores each written index use and
its normalized `ShuffleLane::Index` or `ShuffleLane::Undefined` selection. Those
uses have `UseContext::UnevaluatedValue`; bounds and type operands inside them are
also suppressed. A dynamic mask uses its second ordinary value operand.

The Clang query side-effect gate still examines the written index children. For
example, an index containing `__builtin_constant_p(n++)` does not increment `n`,
but can suppress an enclosing object-size query's scalar fallback. Native tests
cover this distinction. Query values retain Toucan's existing conservative
constant-knowledge policy; this feature does not add vector constant folding.

## Current boundaries

The existing vector policy limits inputs and results to 16 bytes. Results must
have a power-of-two lane count. GNU requires that count; valid odd-lane Clang
results receive an explicit unsupported-layout diagnostic until their padded
storage and cast rules are implemented. GNU pointer-valued constant indices also
receive an explicit unsupported diagnostic. No target instruction set is enabled,
and generated Rust still rejects unsupported vector values at call boundaries.

The GNU profiles use GCC 13 semantics; the compiler evidence uses genuine GCC
13.3 on x86-64 and AArch64 and Clang 18 across five targets. Builtin feature-query
macros retain the facade's conservative reporting policy.

The behavior is checked against the pinned
[GCC implementation](https://github.com/gcc-mirror/gcc/blob/releases/gcc-13.3.0/gcc/c-family/c-common.cc),
[Clang type checker](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/Sema/SemaChecking.cpp),
and [Clang lowering](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/CodeGen/CGExprScalar.cpp).
[Validation evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/vector-shuffle-2026-09-08.json) records
commands, hashes, compiler differences, execution checks, and current source blockers.
The [integration report](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/vector-shuffle-integration-2026-09-08.json)
records native tests and retained-graph checks after declaration alignment and
old-style definitions, including charging owned shuffle payloads before allocation.
