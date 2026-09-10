# Typedef alignment and expression types

Clang can preserve a typedef's alignment through an expression even when its
underlying C type is unchanged. The shared typedef ancestry matters:

```c
typedef int A __attribute__((aligned(32)));
typedef int B __attribute__((aligned(32)));
typedef A C __attribute__((aligned(16)));

/* Result type alignments: 32, 4, and 32, respectively. */
typedef __typeof__(__builtin_elementwise_min((A)1, (A)2)) Same;
typedef __typeof__(__builtin_elementwise_min((A)1, (B)2)) Independent;
typedef __typeof__(__builtin_elementwise_min((A)1, (C)2)) Shared;
```

Toucan retains that ancestry for Clang integer arithmetic, integer elementwise
operations, and elementwise vector results. Actual integer promotions discard
the source typedef when they change its canonical type. When integer arithmetic
selects one wider operand type, that operand's ancestry survives. A newly formed
unsigned type has its natural alignment.

Redeclarations keep separate snapshots. An alias created before its underlying
typedef gains stronger alignment retains its earlier alignment. A subsequent
common-type operation can merge the related declarations. Plain intervening
aliases and `typeof` wrappers also matter: two `typeof(type)` layers can merge,
while distinct `typeof(expression)` layers stop that part of the merge.

Object redeclarations use the latest written type's ancestry, including nested
pointer and callback types. Function redeclarations preserve the first visible
function type's ancestry. These rules change observable `typeof` alignment;
array bounds and function contracts still undergo their ordinary composition.

## Library representation

`Type::alignment` is a four-byte `TypeAlignment` value. `bytes()` returns its
optional byte alignment; `origin()` returns an optional `AlignmentOriginId`.
`TypeAlignment::new(bytes)` constructs a byte alignment without asserting a C
typedef identity. `TranslationUnit::typedef_alignment()` keeps its byte-valued
query API, and `typedef_alignment_metadata()` returns the full snapshot.

Origin IDs belong to `TranslationUnit::alignment_origins`. Immutable rows record
parent type layers and canonical typedef/redeclaration links. A row can have no
alignment override: an earlier plain declaration may be related to a later
aligned declaration. IDs do not change C type compatibility or substitute for
record/tag identities. Caller-built units must keep IDs and snapshots consistent;
`validate_alignment_origins()` checks the table and its owned type references.
Binding generation and expression queries perform that validation through their
existing unit validation path.

The slot keeps `Type` at 40 bytes on the measured x86-64 host. Ordinary inputs with
no relevant alignment have no origin arena or registry allocations. A bounded
syntax pass finds names that may gain alignment later; it does not evaluate
attributes or make names visible before their declarations. Origin tables stop
at 65,536 rows, and common-type ancestry walks stop at 128 layers. Retained-code
budgets charge rows and their type-reference edges before allocation.

Across 42 ordinary-source measurements against the preceding commit, allocation
counts were identical. Reusable parser-stack sessions also had identical allocated
bytes; direct analysis entry points used a fixed additional 24 bytes for the
larger owned result. Large synthetic files still expose existing declaration
lookup costs, so this does not claim linear whole-frontend scaling.

Inspection JSON preserves the numeric `alignment` field and adds optional
`alignment_origin` fields plus an `alignment_origins` table. No encoded slot bits
are part of the serialization.

## Boundaries

GNU arithmetic alignment and general floating/complex common-type alignment
remain separate compiler behaviors; previously unsupported forms still produce
explicit diagnostics. Pointer composite alignment remains subject to its
existing limitation. The metadata does not make a Rust alias with different
size/alignment or an unsupported by-value vector ABI representable. Those binding
diagnostics remain in place.

The common-type rule follows the pinned
[Clang 18 ASTContext implementation](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/AST/ASTContext.cpp)
and is tested against compiler results. Type checking and LLVM layout probes are
separate from native execution evidence.

[Validation evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/aligned-ancestry-2026-09-08.json) records
1,580 matching source/target assertions, retained-graph parity, unchanged zstd
translation units, ordinary allocation measurements, and scaling limits.
