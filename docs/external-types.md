# Caller-owned external types

`BindingOptions::blocklist_types`, `toucan bindgen --blocklist-type`, and
`toucan_bindgen::Builder::blocklist_type`
omit C type definitions while preserving references to their Rust names. Patterns
are exact names or prefixes ending in `*` in the library; the builder accepts
exact identifiers and trailing `.*`, with optional anchors. This is the
nonrecursive model described in [bindgen's blocklisting guide](https://rust-lang.github.io/rust-bindgen/blocklisting.html).

Supply the definition or import with `raw_line`, or in the Rust module containing
the generated bindings:

```rust,ignore
let bindings = toucan_bindgen::Builder::default()
    .header("api.h")
    .blocklist_type("Handle")
    .raw_line("#[repr(C)] pub struct Handle { _private: [u8; 0] }")
    .generate()?;
```

This example requires the C API to use `Handle` only through pointers. The C
frontend still checks the original definition. It does not check the supplied
Rust source.

## What the report means

`Report::blocked_types` records the C name, C namespace/kind, deterministic Rust
name, whether collection reached the type from an emitted declaration or alias,
and the known C size and alignment. The field is absent from serialized reports
when no types were blocked. Incomplete types have no known size; an incomplete
array can still have known alignment.

`layout_required` distinguishes complete storage from an opaque pointer use:

- Values, generated fields, and extern objects require matching storage. Toucan
  emits compile-time size/alignment assertions when the C facts are known.
- Pointers to external types can refer to opaque Rust types with different size
  or alignment. Toucan emits no pointee layout assertion for this case. Pass the
  pointer back to C; do not use the opaque Rust type to allocate, index, or read
  the hidden C object.
- An alias alone does not require complete storage. A later value use upgrades
  the requirement, including through other aliases and containing records.

The caller owns the Rust type's representation, valid bit patterns, ownership,
and full calling convention. Matching size and alignment does **not** prove
that a Rust type can safely cross the C call boundary. Toucan continues to reject
known unsupported C call ABIs after substitution, including nested callbacks,
records containing atomic storage or bitfields, vectors, long double, and
unsupported narrow Clang atomic parameters/results. Rust 1.64 rejects exposed
128-bit ABI/storage, but permits an explicitly external opaque pointee that hides
128-bit C storage.

Complex and binary128 call values retain their existing diagnostics. Selected
complex storage also remains unsupported; an explicitly external opaque pointer
can hide complex or binary128 storage without exposing its representation.

## Names, constants, and generated records

A blocked typedef retains its name in other aliases. Blocking an anonymous
record or enum's owning typedef suppresses that anonymous definition. Blocking a
named enum suppresses its constants; blocking only a typedef of a separately
named enum leaves the enum and constants available. These ownership rules are
checked against bindgen 0.72.1. Unrelated tags and typedefs with the same C spelling
retain distinct generated Rust names; use the report to find them.

Generated records containing caller-owned storage do not derive `Copy` or
`Clone`. Union fields use `ManuallyDrop` where the external type might need drop.
Pointer fields do not inherit these restrictions. Generated bitfield accessors
need a generated integer type; external bitfield field types require C accessors.

## Evidence

[The external-type evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/external-types-2026-09-08.json)
records the seven-profile frontend checks, native GCC/Clang calls with C at
`-O0`/`-O2` and Rust at optimization levels 0/3, and current/Rust 1.64 compilers.
The native fixtures cover caller-owned non-`Copy` types, arrays, unions,
callbacks, and C-owned opaque storage containing long double and 128-bit fields.
A separate real zlib 1.3.1 probe compiles untouched `adler32.c` and generates from
untouched `zlib.h`, supplying only the external `Bytef = u8` definition. Its 16
executables each check 28 checksum cases. These are evidence for the exercised
interfaces, not a proof for arbitrary caller-provided Rust types or all targets.

With no type blocklist, the four existing zlib, SQLite, zstd, and libgit2 header
commands produce identical binding bytes before and after this change. Three
allocation samples per header show unchanged allocation counts and 24 additional
requested bytes per generation for the larger returned report. This measurement
covers binding generation with an instrumented allocator, not frontend throughput.
