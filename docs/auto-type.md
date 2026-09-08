# Inferred GNU declarations

Toucan supports a single initialized `__auto_type` variable, including a
parenthesized name, in file, block and `for` declarations. Deduction applies array
and function decay and removes outer ordinary qualifiers from the initializer's
type. Explicit declaration qualifiers, storage classes and supported attributes
then use the normal declaration checks. Void values, incomplete records, bitfields,
brace initializers, typedefs, parameters and member declarations are diagnosed.

GNU profiles introduce the name after its initializer. This permits a local
`__auto_type x = x + 1` to read an outer `x`. Clang profiles reject references to
the undeduced name, including references through `sizeof`, `typeof`, casts and
unselected expression arms. Nested declarations can still shadow that name.
The complete initializer is checked before binding the deduced variable, so
ordinary and retained analysis resolve those references identically.

GNU deduction removes an outer atomic wrapper. Clang preserves it, including an
atomic pointer to a variable length array. Clang also applies written `restrict`
to an inferred non-pointer type; its resulting qualifier is retained. A Clang
file declaration that completes an earlier object declaration uses its established
type and checks the initializer conversion to that type.

The checked API exposes `DeclarationSite::type_inference()`. It records the mapped
keyword span, the original initializer expression, the inferred `TypeUseId`, and
whether Clang reused an earlier declaration's type. This is a type dependency;
the declaration's initializer remains the single execution site. Existing VLA
bound identities survive decay, atomic conversion and declaration qualifiers.
Parsing, expression nesting and retained graph quotas apply to inferred
declarations too. With retention disabled, inference metadata is not allocated.

Clang's derived and multiple declarators have explicit unsupported diagnostics
in this first layer. GCC rejects those forms. Explicit `_Atomic __auto_type` is
supported for GNU profiles; Clang 18 leaves `_Atomic(__auto_type)` undeduced in its
AST, accepts incompatible wildcard type comparisons, and crashes on `sizeof`
even in syntax-only mode. Toucan diagnoses that Clang form instead of inventing
a concrete type. Plain Clang inference from an already atomic initializer is
supported and tested. Exact atomic rvalue copies preserve that type without an
atomic load; ordinary atomic lvalue initializers retain their load conversion.

Clang 18 can incorrectly evaluate a bound in an unselected conditional arm,
including in ordinary declarations without `__auto_type`. Toucan retains C11's
selected-arm semantics. The [compiler-oracle discrepancy](compiler-oracle-discrepancies.md)
is recorded separately from frontend conformance.

The implementation follows [GCC's `__auto_type` contract](https://gcc.gnu.org/onlinedocs/gcc/Typeof.html)
and pinned [GCC 13.3 declaration parsing](https://github.com/gcc-mirror/gcc/blob/releases/gcc-13.3.0/gcc/c/c-parser.cc),
[Clang 18.1.3 deduction](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaTemplateDeduction.cpp)
and [declaration checking](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaDecl.cpp).
[Saved probes](../corpus/evidence/auto-type-2026-09-08.json) include source text,
compiler versions, syntax and runtime results, and the explicit Clang deduction
failure. Native tests check GCC and Clang at `-O0` and `-O2`; cross-target tests
check all five Clang backends and all five Toucan target profiles.
