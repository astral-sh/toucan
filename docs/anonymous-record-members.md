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

## Validation

The maintained [Microsoft member tests](../crates/toucan_semantic/tests/ms_anonymous_members.rs)
check source forms, scope, completeness, initialization, and retained references.
[Direct qualifier tests](../crates/toucan_semantic/tests/direct_anonymous_qualifiers.rs)
compare compiler-specific layout and assignment constraints. Their native
oracles distinguish ordinary C rejection from compiler failures.
[Generated-layout tests](../crates/toucan/tests/ms_anonymous_members.rs) execute
on the native host and compile Windows target assertions. Windows compilation
checks do not establish Windows execution compatibility.

The [Microsoft member capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/ms-anonymous-members-2026-09-09.json.gz)
and [direct qualifier capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/direct-anonymous-qualifiers-2026-09-09.json.gz)
retain the original GCC/Clang observations and Rust compilation results. The GNU
atomic anonymous-member layouts remain an explicit support gap described above.
These captures apply to their recorded compiler versions and source revisions.
