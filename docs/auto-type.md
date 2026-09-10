# Inferred GNU declarations

Toucan supports initialized `__auto_type` variables in file, block and `for`
declarations. GNU profiles require one plain identifier (possibly parenthesized).
Clang profiles also support declaration groups and pointer, array and function
pointer declarators. Deduction applies array
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
type and checks the initializer conversion to that type. Such a redeclaration
does not determine the new deduced base shared by the rest of its group.

For example, Clang accepts:

```c
int n;
__auto_type value = 1, *pointer = &n;
int callback(int);
__auto_type (*call)(int) = callback;
```

Each group item is checked in source order, so later initializers can use earlier
variables. Common declaration attributes are checked once. The deduced base types
must be identical, including qualifiers, enum identity, function prototypes and
[VLA type identities](vla-type-identity.md). Typedef and direct spellings of the
same qualified array remain equal.

Written array bounds must match the initializer's fixed or incomplete array type;
a written runtime-bound array pattern cannot be deduced. The deduced base itself
can be a VLA: `int a[n]; __auto_type *p = &a;` preserves its existing bound.
Function declarator patterns require matching prototypes, parameter types,
variadic status and calling convention. A plain pointer pattern can still infer
an existing function type without a prototype.

GNU and Clang profiles also admit their array-element qualification extension,
such as `const int (*p)[3] = &array` for an `int array[3]`. Bounds and nested pointer
element types remain constrained; qualifier removal is diagnosed. GCC and Clang
reject this extension under strict C11 `-pedantic-errors`.

The checked API exposes `DeclarationSite::type_inference()`. It records the mapped
keyword span, the original initializer expression, the inferred base `TypeUseId`,
and whether Clang reused an earlier declaration's type. The declaration site's
type includes the written pointer, array and function layers. This is a type dependency;
the declaration's initializer remains the single execution site. Existing VLA
bound identities survive decay, atomic conversion and declaration qualifiers.
Parsing, expression nesting and retained graph quotas apply to inferred
declarations too. With retention disabled, inference metadata is not allocated.

Explicit `_Atomic __auto_type` is supported for GNU profiles. Clang 18 leaves
`_Atomic(__auto_type)` and atomic derived-pointer patterns undeduced in its AST,
accepts incompatible wildcard type comparisons, and crashes on dependent `sizeof`
queries even in syntax-only mode. Toucan diagnoses that Clang form instead of inventing
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
[Saved probes](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/auto-type-2026-09-08.json) include source text,
compiler versions, syntax and runtime results, and the explicit Clang deduction
failure. Native tests check GCC and Clang at `-O0` and `-O2`; cross-target tests
check all five Clang backends and all five Toucan target profiles.

[Derived declaration probes](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/auto-declarators-2026-09-08.json)
record the additional Clang/GCC constraints, qualified array spellings, bounds,
callback calls at `-O0`/`-O2`, and normal/retained parity across all seven profiles.
The 128-level declarator/type limits and retained graph quotas also cover these
forms. Parsing does not create a placeholder semantic type.

Against the preceding VLA-identity layer, four ordinary declaration allocation
fixtures and all four library binding hashes were unchanged. Five alternating
release measurements put normal header analysis within −0.1% to +1.6% and retained
analysis within −6.8% to +1.0% of that baseline. Seven alternating binding runs
were within −2.3% to −0.2%. These shared-host observations check for regressions;
they do not establish a speedup. The evidence includes the exact baseline,
commands, output hashes and individual observations. Generated deduced globals
and callback bindings compiled and ran with Rust 1.64 and current Rust.
