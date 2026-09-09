# Builder name allowlists

The Builder supports `allowlist_type`, `allowlist_function`, and `allowlist_var`
with Rust regular expressions anchored to the whole name. All name and physical
file patterns form a union. Required types are included; no match selects no
roots, and invalid expressions produce configuration errors.

Type roots keep C's tag namespace separate from function/object names. Nested
tags use the emitter's lexical names, such as `Outer_Inner`. Variables and
functions match each eligible occurrence's callback-adjusted name. Function
redeclarations retain the first selected name. Objects retain the selected
occurrence's checked initializer and type. Macros match their original names and
retain the existing ordered definition context. Anonymous top-level enum
constants without a typedef can select their entire enum; named and nested enum
constants do not independently root their enum through `allowlist_var`.

`BindingSelection::extend_tags` reuses the emitter's bounded lexical naming
implementation. The opt-in `retain_type_dependencies` policy retains referenced
types when a selected function is static or blocklisted, and traverses reached
blocklisted definitions without emitting those definitions. Visited type IDs and
alias names bound recursive and shared dependency expansion. Source parameter
aliases use the existing optional catalog and the selected occurrence offsets;
these aliases do not alter adjusted C parameter types. Default core and file-only
selection do not enable the new dependency-retention policy.

## Native evidence

The [saved Linux x86-64 capture](../corpus/evidence/name-allowlists-2026-09-09.json.gz)
contains the original 70-case bindgen 0.72.1/libclang 18.1.3 study, its corrected
candidate comparison, and 32 additional cases. All 70 original public-export
inventories match. The initial study's regex omitted `pub use` aliases; the
corrected comparison includes them while preserving the original observations.
The extension covers stateful object callbacks, macro history, cyclic/shared
dependencies, callback parameter aliases, real lexical name collisions, and four
zstd 1.5.7 header selections.

Across the 102 rows, 96 normalized public API comparisons match. Four of these
supply explicit consumer definitions for intentionally blocklisted types. The
normalization preserves signatures, pointer mutability, callback nullability,
public fields, constants and aliases. It treats Linux C link names with bindgen's
leading U+0001 marker as the same linker symbol, and ignores opaque private
placeholder field spellings. Raw analyzer outputs and normalization rules are
retained. Ninety-seven generated Rust layout/value probes agree.

The remaining rows have explicit qualifications:

- Two Rust enum rows compile, but the shared API analyzer does not model Rust
  enums. Their selected export inventories match.
- One bare function typedef captures the initial non-null versus `Option`
  difference. The saved comparison retains that observation; the subsequent
  [function-typedef layer](function-typedef-bindings.md) matches bindgen's
  nullable callback aliases.
- Two cases select multiple callback names for a single object. The captured
  initial layer diagnoses this unsupported projection. The subsequent
  [multiple-object-name layer](multiple-object-names.md) supports both exports.
- One selected lexical type-name collision is diagnosed by the Builder. Native
  bindgen emits duplicate Rust definitions that fail Rust compilation.

A real zstd compression/decompression roundtrip links both generators' selected
bindings to the pinned zstd 1.5.7 static library on current Rust and actual Rust
1.64. GCC and Clang C executions agree with all four Rust executions. Eight more
Rust executions link callback-renamed functions and a global against GCC/Clang
objects on both Rust versions. Nine focused regression tests pass, including a
Rust compilation test. These are selection and ABI checks, not a performance or
full bindgen API compatibility claim.

A subsequent regression checks a collision between a type-selected enum constant
and a variable-selected macro. Collision validation runs after all explicit type
roots have contributed their constants, so generation rejects the conflicting
Rust name.
