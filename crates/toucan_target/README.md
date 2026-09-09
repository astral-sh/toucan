# toucan_target

Target descriptions and C object layouts for Toucan.

This crate computes layouts without invoking a compiler or loading libclang. It wraps
[`repc`](https://github.com/mahkoh/repr-c) with explicit target profiles, serializable input and
output types, and validation for unsupported inputs.

## Targets

| Target | `char` | `long` | `wchar_t` | `long double` size / alignment |
| --- | --- | --- | --- | --- |
| `x86_64-unknown-linux-gnu` | signed | 64 bits | signed, 32 bits | 16 / 16 bytes |
| `aarch64-unknown-linux-gnu` | unsigned | 64 bits | unsigned, 32 bits | 16 / 16 bytes |
| `x86_64-apple-darwin` | signed | 64 bits | signed, 32 bits | 16 / 16 bytes |
| `aarch64-apple-darwin` | signed | 64 bits | signed, 32 bits | 8 / 8 bytes |
| `x86_64-pc-windows-msvc` | signed | 32 bits | unsigned, 16 bits | 8 / 8 bytes |

All supported profiles have eight-bit bytes, little-endian storage, and 64-bit pointers.
Target selection never falls back to the build host. Compiler options that change the ABI,
including `-fshort-enums`, `-fpack-struct`, and `-funsigned-char`, are not part of these profiles.

## Layouts

`Target::layout` accepts a `Type` containing scalars, structs, unions, arrays, enumerations,
and typedefs. Records support bitfields, zero-width barriers, GNU packing/alignment attributes,
and `#pragma pack`. Annotations use **bits**: `PragmaPack(16)` means `#pragma pack(2)`.

Results retain object size, pointer alignment, field alignment, MSVC required alignment, and
field offsets in bits. Ordinary fields and bitfields use the same offset representation;
unnamed bitfields have no addressable field layout. Nested records can be queried separately.

The semantic layer must validate C declaration constraints, including flexible array placement,
tag completeness, and whether a declaration can define an object. This crate rejects void object
layouts, noninteger bitfields, oversized boolean bitfields, invalid packing values, unsupported
MSVC 128-bit integers, and type nesting beyond 256. The ABI engine checks alignment validity,
bitfield widths, and size overflow.

`long double` uses the explicit profile above because `repc` does not expose a corresponding
scalar type. Its object layout does not imply that Rust can pass or return that value by value.

`Target::predefined_macros` supplies a deterministic subset of compiler macros for C11 header
processing. These describe the selected target and the frontend's compatibility profile; they
do not query an installed compiler. Feature queries and source-dependent macros belong to the
preprocessor.

## Validation

```console
cargo test -p toucan_target
cargo test -p toucan_target --test c_probe -- --include-ignored
cargo clippy -p toucan_target --all-targets -- -D warnings
```

The compiler probes require Clang and a native C compiler (`CC` or `cc`). They compare C scalar
and record sizes, alignments, and field offsets across all five targets using compile-time
assertions without a target sysroot. A separate native probe sets ordinary and packed bitfields,
then checks the resulting object bytes against the computed bit offsets. It runs on supported
Linux and macOS hosts. Cross-compilation does not establish native execution on another OS.
