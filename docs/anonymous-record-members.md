# Anonymous record members

A directly written unnamed struct or union declares an anonymous member. A
`typeof` specifier without a declarator does not. For example:

```c
struct { int member; } source;
struct Owner { __typeof__(source); int field; };
```

GCC and Clang ignore the `typeof` declaration. `Owner` has one field at offset
zero and has the size of `int`. Toucan preserves that layout instead of adding
storage for `source`'s type. Direct anonymous structs and unions retain their
member storage and promoted field names.

## Qualifiers on direct anonymous members

For a directly written anonymous struct or union, GNU preserves `const` and
`volatile` on the member. Clang discards the outer qualifiers, including a written
`_Atomic` qualifier. Qualifiers on the record's own named fields remain in effect,
and both compiler families reject `restrict` on these record declarations.

GNU atomic anonymous members currently return an explicit unsupported diagnostic.
Their atomic storage cannot be erased: for an anonymous struct containing two
`int` fields followed by another `int`, GCC produces size 16 and alignment 8;
Clang produces size 12 and alignment 4. Toucan rejects the unsupported GNU form
before returning a translation unit or generating bindings. Supporting it also
requires atomic-aware initializer and promoted-member paths.

The [direct qualifier evidence](../corpus/evidence/direct-anonymous-qualifiers-2026-09-09.json.gz)
records GCC/Clang layout and assignment controls, 704 native checks, and ordinary/
retained parity across 1,408 profile/mode/qualifier configurations. The 64 native
GNU atomic layout observations remain an explicit frontend support gap. Clang
bindings retain their target guards and pass layout compilation with Rust 1.64
and 1.98.1 for Linux and Windows; the Linux bindings also execute their generated
layout tests on the native host.

The [native evidence](../corpus/evidence/anonymous-typeof-member-2026-09-09.json.gz)
compares GCC and Clang and checks all Clang physical targets.

## Microsoft anonymous members

The Windows MSVC profile also admits a named struct or union, or a typedef that
denotes one, without a declarator:

```c
typedef struct Named { int value; } Alias;
struct FromAlias { Alias; int field; };
struct FromTag { struct Named; int field; };
```

Both containing records have size 8, alignment 4, and `field` at offset 4 on
`x86_64-pc-windows-msvc`. Their promoted `value` member occupies offset 0. On
the Linux profiles, these declarations add no member; the records have size 4
and `field` at offset 0. A named definition written inside the containing record
follows the same target-specific rule. All eight supported C modes use these
non-pedantic compiler semantics.

The embedded storage uses the canonical record. Clang ignores qualifiers and
extra alignment carried by the typedef, and declaration attributes written on
the anonymous field. Attributes on the record tag itself still determine its
layout. For example, a `const` record typedef does not make the promoted member
const; a const-qualified member inside the record remains const.

Source form matters. A scalar, pointer, array, or atomic typedef adds no anonymous
member. Neither `typeof(record)` nor `_Atomic(RecordAlias)` adds a member. A
written `_Atomic` qualifier beside an ordinary record alias is ignored, and the
alias still supplies an ordinary anonymous record member. Local typedefs are
resolved at their lexical scope before applying these rules.

Admitted members use the usual completeness, duplicate-name, layout,
initialization, and member-lookup checks. Retained code preserves both the
canonical storage field and references to the written typedef. The shared
source-form classifier also distinguishes aliases requiring record resolution
from directly written record specifiers.

The implementation follows Clang 18's
[anonymous member admission and construction](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaDecl.cpp).
The [Microsoft member evidence](../corpus/evidence/ms-anonymous-members-2026-09-09.json.gz)
includes reproducible sources, compiler commands, and raw results. The focused
tests compare ordinary and retained analysis for 19 source forms
across 11 compiler profiles and eight language modes. Recorded Clang checks cover
304 valid Linux/Windows combinations and 12 additional constraint, scope,
initializer, and storage cases. Windows checks compile C assertions using the
cross target on Linux; they do not execute a Windows program. A separate Clang
layout-dump tool failure encountered during preliminary inspection is excluded
from the conformance results.

Generated bindings compile and pass their layout tests on the native Rust host
with Rust 1.64 and Rust 1.98.1. The unchanged Windows bindings are also compiled
with both versions' Windows standard libraries, retaining the generated target
guard. Eight cross-compilation checks verify constant size/alignment assertions
and field offsets in optimized LLVM output for the four member forms. These are
Windows compilation checks, not Windows execution results.

The [combined-source validation](../corpus/evidence/ms-anonymous-members-root-integration-2026-09-09.json.gz) repeats the semantic and native checks, compiles and runs the host bindings with Rust 1.64 and 1.98.1, and compiles all four Windows layouts with both toolchains. Workspace Clippy passes.

The [combined qualifier validation](../corpus/evidence/direct-anonymous-qualifiers-root-integration-2026-09-09.json.gz) repeats all focused native and retained-analysis checks. Generated Clang layouts compile for Linux and Windows under Rust 1.64 and 1.98.1; both Linux layout tests execute. Workspace Clippy passes.
