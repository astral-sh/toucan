# GNU type introspection

Toucan supports `__builtin_types_compatible_p(type, type)` and
`__builtin_choose_expr(condition, expression, expression)` in GNU and Clang C
headers and function bodies.

The type query returns an `int` integer constant expression. It ignores outer
`const`, `volatile`, and `restrict` qualifiers, including qualifiers on an array's
element type. Qualifications behind pointers remain significant. Array bounds,
record and enum identity, compatible enum integer types, function prototypes, and
calling conventions use the frontend's ordinary compatibility rules. GNU profiles
ignore ordinary qualifiers on function return types; Clang profiles retain them.
An incomplete enum is incompatible with integer types until its definition selects
one, except on Microsoft ABI targets where an unqualified forward enum already
matches `int`. Enum and integer types with ordinary qualifiers remain distinct in
every profile, including through pointers and qualified array typedefs. The query
still strips ordinary qualifiers from its outer type operands.
Both written types are checked, including their bound expressions and declarations,
but their runtime bounds and `typeof` operands do not execute.

The selection condition must be an integer constant expression. Both arms are
checked for invalid names, types, calls, and statement constraints. Only the selected
arm executes. Its type, lvalue or function category, bitfield width, and register
status are preserved without the usual conditional-operator conversions. Constant
eligibility follows that arm, including floating and address initializers. The
condition and discarded arm suppress runtime bounds and `typeof` effects. An
already-declared VLA typedef retains its earlier bound evaluation.

The checked API retains distinct `ExprKind::TypesCompatible` and `ExprKind::Choose`
operations. Type queries link both written type occurrences and `TypeUseId`s;
selections link all three expressions and record the selected arm. These source
links remain available even for unevaluated operands. Constant-query side-effect
summaries inspect only the selected arm, preserving the existing conditional Clang
query evaluation policy. For example, Clang can evaluate the VLA bound in
`__builtin_constant_p(__builtin_choose_expr(1, sizeof(int[n++]), n++))`; the discarded
increment does not suppress it. The query's integer result can differ between
optimization levels.

The implementation follows the [GCC builtin documentation](https://gcc.gnu.org/onlinedocs/gcc/Other-Builtins.html)
and pinned compiler sources: Clang 18.1.3
[`BTT_TypeCompatible`](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaExprCXX.cpp),
[`ActOnChooseExpr`](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaExpr.cpp),
and [selected-arm effect checking](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/AST/Expr.cpp).
The semantic tests compare constraints and runtime bound effects with GCC and
Clang at `-O0` and `-O2`, check all five target profiles, and compare ordinary and
retained declarations and diagnostics. Selection and type-query caches have a
65,536-entry limit each; parser and expression depth/work limits also apply.

[Saved compiler probes](../corpus/evidence/type-introspection-2026-09-08.json) include
source text, compiler versions, runtime observations, and primary-source hashes.
GNU compatibility queries ignore an outer atomic wrapper, including through the
outer array-element chain. Clang preserves it. Both preserve atomic identity below
pointers and in function returns. The compiler matrix checks these differences.
GNU profiles also distinguish atomic enums from their atomic compatible integer
types below pointers and in function types; Clang profiles compare their contained
types. Atomic qualification does not extend through a contained pointer.
