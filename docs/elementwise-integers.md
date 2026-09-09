# Elementwise integer operations

The Clang 18 profile supports `__builtin_elementwise_add_sat`,
`__builtin_elementwise_sub_sat`, and integer `__builtin_elementwise_min` and
`__builtin_elementwise_max`. Saturating addition and subtraction clamp each result
to the signed or unsigned element type's range. Integer min/max select a value
using that type's ordering.

Scalar operands undergo Clang 18's usual arithmetic conversions. Two signed-char
operands therefore produce an `int`, and `add_sat((signed char)127, (signed
char)127)` returns 254. Vector operands must have identical types; each lane
keeps its original width, so signed 8-bit vector addition clamps at 127. Scalar
operands are not broadcast into vectors. Atomic and volatile lvalue reads remain
ordinary argument evaluations.

Retained calls use `Builtin::Elementwise(ElementwiseOperation)` with two converted
value operands. Both execute once, in C's unspecified argument order. The
intrinsic itself has no memory effects; query side-effect checks inspect the
operands. Clang 18 does not make these calls C integer constant expressions or
prove literal calls constant through `__builtin_constant_p`. No scalar or vector
constant evaluator is added here.

These rules describe the pinned Clang 18 profile. Newer Clang documentation has
changed scalar promotion and constant-evaluation rules; it does not establish
compatibility with those newer compiler versions. GNU 13 profiles diagnose the
unsupported intrinsic names. The
[Clang 18 semantic checker](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/Sema/SemaChecking.cpp)
and [Clang 18 lowering](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/CodeGen/CGBuiltin.cpp)
are the source references used by this layer.

## Remaining limits

Floating min/max have an explicit unsupported diagnostic. Their IEEE minNum
behavior, including NaNs, needs separate validation. Vector storage retains the
existing 16-byte limit; this layer establishes no Rust vector calling convention.

Aligned typedef operands retain their common typedef ancestry, including casts,
intervening aliases, and declaration-time alignment snapshots. Independently
declared aliases can therefore have a different result alignment from two uses
of the same alias. See [type alignment](type-alignment.md) for the owned metadata
and remaining compiler-specific boundaries.

[Validation evidence](../corpus/evidence/elementwise-integers-2026-09-08.json)
separates matching compiler decisions from those valid-source limitations.
