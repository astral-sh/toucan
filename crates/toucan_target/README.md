# toucan_target

Target profiles, C object layouts, and predefined macros for Toucan. This crate
wraps [`toucan_layout`](../toucan_layout) with serializable input and output types
and layout validation. It uses the selected target independently of the build
host, without invoking a compiler or loading `libclang`.

## Targets

| Target | `char` | `long` | `wchar_t` | `long double` size / alignment |
| --- | --- | --- | --- | --- |
| `x86_64-unknown-linux-gnu` | signed | 64 bits | signed, 32 bits | 16 / 16 bytes |
| `i686-unknown-linux-gnu` | signed | 32 bits | signed, 32 bits | 12 / 4 bytes |
| `armv7-unknown-linux-gnueabihf` | unsigned | 32 bits | unsigned, 32 bits | 8 / 8 bytes |
| `aarch64-unknown-linux-gnu` | unsigned | 64 bits | unsigned, 32 bits | 16 / 16 bytes |
| `x86_64-unknown-linux-musl` | signed | 64 bits | signed, 32 bits | 16 / 16 bytes |
| `aarch64-unknown-linux-musl` | unsigned | 64 bits | unsigned, 32 bits | 16 / 16 bytes |
| `x86_64-apple-darwin` | signed | 64 bits | signed, 32 bits | 16 / 16 bytes |
| `aarch64-apple-darwin` | signed | 64 bits | signed, 32 bits | 8 / 8 bytes |
| `x86_64-pc-windows-msvc` | signed | 32 bits | unsigned, 16 bits | 8 / 8 bytes |
| `aarch64-pc-windows-msvc` | signed | 32 bits | unsigned, 16 bits | 8 / 8 bytes |

All targets have eight-bit bytes and little-endian storage. Pointers are 32 bits
on i686 and ARMv7, and 64 bits elsewhere.

`CompilerProfile::new(target, compiler)` selects GCC or Clang and rejects
unsupported pairs. Linux defaults to GCC except on ARMv7, which supports Clang
only. Darwin and Windows use Clang; Windows follows the Microsoft ABI. See
[compiler profiles](../../docs/compiler-profiles.md) for supported combinations
and the limits of each profile.

## Layouts

`Target::layout` uses the target's default compiler; `CompilerProfile::layout`
uses the selected compiler throughout the type tree. Both accept scalars,
structs, unions, arrays, enums, and typedefs, including bitfields and packing or
alignment annotations.

Results contain object size, alignments, and field offsets in bits. Annotation
arguments also use bits: `PragmaPack(16)` means `#pragma pack(2)`. This crate checks
layout constraints; callers must check C declaration rules such as flexible array
placement and tag completeness. An object layout alone does not establish how
the type can be passed or returned in a function call.

`Target::predefined_macros` and `CompilerProfile::predefined_macros` provide a
deterministic subset of compiler macros for header processing. Profile macros
reflect the selected target, compiler, and [C language mode](../../docs/language-modes.md).

## Validation

```console
cargo test -p toucan_target
cargo test -p toucan_target --test c_probe -- --include-ignored
```

The ignored compiler probes require GCC, Clang, and a native C compiler (`CC` or
`cc`).
