# Object declaration type spelling

Optional object-value capture preserves the typedef spelling written on each
compatible object redeclaration. For `extern First shared; Second shared = 7;`, a
selected second occurrence uses `Second`; the canonical C declaration and checked
initializer remain unchanged. The core frontend does no additional copying when
object capture is disabled.

The written type is used only when it has the same complete type information.
Earlier array bounds and function prototypes remain in effect when a later
spelling omits them. For example, `First[4]` followed by an incomplete `Second[]`
keeps the completed four-element type. Compatible nested pointer and callback
aliases follow the same rule.

`ObjectTypeComparison` removes typedef spelling for validation, preserving
qualifiers, bounds, prototypes, calling conventions, parameter contracts, and
nominal record, enum, and vector identities. Array qualification applies to the
element; function parameter top-level qualification is not part of the function
type. Alignment provenance does not establish C type identity: existing Rust
storage and calling-ABI checks still reject unrepresentable aligned aliases.
Occurrence names and type IDs retain their existing same-analysis ownership
requirement; structural validation does not establish cross-analysis identity.

## Resource limits

The pre-composite type snapshot is charged to the existing 64 MiB object-value
metadata budget before cloning, including snapshots later discarded in favor of
completed types. The 100,000-occurrence and 128-level type-copy bounds still apply.
Comparisons share a one-million-reference allowance across an entire capture
pass and across an entire binding-validation pass, including parameter-contract
entries. The immutable comparator caches successful type pairs to avoid repeated
walks through shared callback typedefs. Its borrowed type arguments keep cached
identities alive and immutable. Comparison nesting is limited to 128 levels.

## Validation and remaining boundaries

The [reference capture](../corpus/evidence/object-occurrence-types-2026-09-09.json.gz)
covers 19 C headers accepted by GCC 13 and Clang 18.
All 35 supported individually selected occurrences match bindgen 0.72.1 typedef
inventories and compile with both current Rust and Rust 1.64. Five linked C cases
cover scalar values, arrays, completed arrays, pointer storage, and callback calls
with both C compilers and both Rust compilers, using both generators.

Three reference selections retain existing explicit limitations: two aligned
scalar aliases have no matching Rust alias ABI; selecting the earlier
nonprototype callback occurrence after a later prototype is rejected. Selecting
the later complete callback is supported. Four supported projections differ in
global mutability: Toucan emits read-only statics for const typedefs, while
bindgen emits mutable statics. GCC and Clang reject assignments to these const
objects; those controls remain in the capture. The other 31 normalized APIs match.

The capture's original source predates the independent
[array-typedef qualifier fix](array-typedef-qualifiers.md). A separate composition
control preserves canonical declarations with optional metadata and checked code,
retains the selected `First` or `Second` alias, and passes eight Rust compilation
checks. The integrated Builder regression also exercises each name separately
and both names together for scalar and qualified-array redeclarations. No timing
claim is made here.

A [Builder composition replay](../corpus/evidence/qualified-object-composition-2026-09-09.json.gz)
checks the first name, second name, and both names together on the integrated
source. All three selections retain their written aliases; twelve Rust
compilations and four GCC/Clang-linked executions pass, including a check that
both names address the same C array. Bindgen exposes the const-typedef first
occurrence as mutable in two selections, while Toucan preserves its const
qualification. The capture retains this API difference.
