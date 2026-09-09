# Enum tag alignment

GNU profiles accept valid enum tag `aligned` attributes and ignore their
alignment, matching GCC 13. Clang profiles outside the Microsoft ABI accept an
explicit alignment on a completed enum when it equals the enum's natural
alignment. This includes `enum E { A = 1 } __attribute__((aligned(4)))` and
`enum __attribute__((packed, aligned(1))) Byte { B = 1 }`.

These forms keep the existing scalar representation, record fields, arrays,
and pointer and value calls. Repeated alignment arguments use their maximum.
Attributes after an existing complete tag remain ignored. Attribute arguments
still require integer constant expressions and valid powers of two. GCC accepts
`aligned(0)` with a warning; Clang rejects it. C11 `_Alignas(0)` remains valid.

## Storage boundaries

Clang alignment-changing enum tags and aligned forward enum declarations produce
explicit errors. A forward declaration cannot establish that alignment will be
redundant: a later packed definition can select a smaller integer.

All Microsoft enum tag alignments remain unsupported. Even `aligned(4)` on a
four-byte enum changes field placement inside packed records on that ABI.
Both GNU attribute spelling and `__declspec(align(4))` impose this requirement.

For a Clang enum with four-byte integer storage and `aligned(16)`, `sizeof` stays
four while alignment becomes sixteen. Clang rejects arrays of this enum and
uses sixteen-byte alignment for loads through enum pointers. Direct value calls
still use a scalar integer ABI. A Rust aligned wrapper would increase its size;
an ordinary integer alias would lose the pointer and field alignment. Lowered
alignment requires separate storage and value-call handling too. Supporting
these forms needs that distinction throughout semantic layout and binding
generation; this change does not approximate them with either representation.

## Attribute cursor visibility

A tag defined in an enum's trailing attribute is hidden from the Builder until
a later type use exposes it. For
`enum E { VALUE = 1 } __attribute__((aligned(sizeof(enum Other { OTHER = 1 }))));`,
the Builder emits `E` and `E_VALUE`. A tag in an interior or prefix attribute
is visited before the enum cursor and remains visible. Sparse discovery records
this distinction even without another enum expression in the file. C lookup,
constant values, and core-default integer projections retain `Other` and `OTHER`.

## Evidence

The [alignment capture](../corpus/evidence/neutral-enum-alignment-2026-09-09.json.gz)
records GCC 13 and Clang 18 size/alignment, packed and pragma-packed records,
array constraints, compatible pointer types, and actual calls using valid enum
objects. LLVM captures separate scalar call signatures from pointer load
alignment. Windows and additional Linux/Darwin targets are compile-only checks.
Pinned bindgen 0.72.1 comparisons cover enum attribute cursor visibility;
generated Rust uses both current Rust and actual Rust 1.64 consumers.

The [combined-source checks](../corpus/evidence/neutral-enum-root-integration-2026-09-09.json.gz) pass the semantic, packing, discovery, and native C/Rust tests, including Rust 1.64 calls. All 101 discovery reference cases are accepted and match public-name sets. Workspace Clippy passes.
