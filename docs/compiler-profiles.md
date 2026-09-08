# Compiler profiles

Select the physical C target and compiler behavior independently:

```console
toucan bindgen api.h --target x86_64-unknown-linux-gnu --compiler clang -o bindings.rs
toucan inspect api.h --target aarch64-unknown-linux-gnu --compiler clang --checked-code
```

Toucan does not invoke an installed compiler. `--compiler` selects the frontend's
implemented semantics, builtin signatures, predefined macros, and layout rules.
Include paths and sysroots remain explicit inputs. Choose the compiler family
used to build the C library: compiler differences can affect its call ABI as well
as header acceptance and record layout. For example, narrow atomic calls supported
by the GNU profile cannot be used with a Clang-built library; see
[atomic call boundaries](compatibility.md#c11-atomic-types).

| Physical target | Default | Other supported compiler |
| --- | --- | --- |
| x86_64-unknown-linux-gnu | GCC | Clang |
| aarch64-unknown-linux-gnu | GCC | Clang |
| x86_64-unknown-linux-musl | GCC | Clang |
| aarch64-unknown-linux-musl | GCC | Clang |
| x86_64-apple-darwin | Clang | — |
| aarch64-apple-darwin | Clang | — |
| x86_64-pc-windows-msvc | Clang with the Microsoft ABI | — |

Musl targets select their own libc environment and generated Rust guards. Use
[the musl validation guide](musl.md) for sysroot setup and the distinction between
native x86-64 and emulated AArch64 runtime evidence.

Unsupported pairs fail before input preprocessing. Compiler selection does not
change the operating system, scalar widths, signedness of plain `char` or
`wchar_t`, long-double encoding, or generated Rust target guard. For example,
Clang on AArch64 Linux still uses binary128 `long double`; it does not inherit
Apple's binary64 format. GCC and Clang can place the same Linux bitfields
differently, so layout uses the selected compiler through nested types.

## Library configuration

```rust
use toucan::{Compiler, CompilerProfile, Config, Target};

let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang)?;
let mut config = Config::with_profile(profile);
config.preprocessor.include_dirs.push("include".into());
config.preprocessor.defines.insert("API_BUILD".into(), "1".into());
# Ok::<(), toucan::target::LayoutError>(())
```

`Config::new(target)` preserves the defaults above. Construct the profile before
customizing its preprocessor settings: `Config` exposes `target()`, `compiler()`,
and `profile()` getters, and has no setter that would overwrite caller macros.

For already-preprocessed C, use
`semantic::analyze_with_profile(source, profile, &options)`. The existing
`analyze` and `analyze_with_options` functions use the target default. The resulting
`TranslationUnit` stores both identities; later layout and constant evaluation
use that compiler, including when evaluating binding macros. Manually modified
translation units reject unsupported pairs through `profile()`, layout, and
evaluation queries.

`Target::layout` and `Target::predefined_macros` retain their default behavior.
Use the corresponding `CompilerProfile` methods for an explicit choice. Public
x86 intrinsic signature and immediate-constraint queries and character-literal
decoding likewise provide `*_with_profile` methods.

## Header compatibility and limits

Compatibility version macros remain conservative: GNU 4.2.1 and, for Clang,
Clang 4.0. They select supported header branches; they do not claim every feature
of those releases or match the installed validation compiler. Capability queries
such as `__has_builtin` retain their existing conservative policy. Compiler
resource headers must match the selected compiler family. Preprocessed input
must be checked with the family that produced it.

The Windows default now defines `__clang__` and its version markers consistently
with its Clang semantics. Clang forward record-tag alignment and packing are also retained on
Windows, correcting an earlier omission. Other existing default behavior is
preserved. Flags such as `-fshort-enums`, optimization, optional instruction sets,
and arbitrary compiler versions are not implied by a profile. Unsupported
extensions continue to produce diagnostics. The layout adapter can represent
aligned enum layouts, but source-level enum alignment attributes remain unsupported; packed enum attributes are supported.

The [profile evidence](../corpus/evidence/compiler-profiles-2026-09-08.json) records
source and binary hashes, compiler commands, allocation samples, and the
Clang zstd `__bf16` blocker observed at that revision. Subsequent [half-type](half-types.md) support resolves those declarations. The seven-profile tests compare ordinary and retained analysis, native and
cross-target C layout probes, unchanged Clang `stdatomic.h` operations, and
actual C/Rust bitfield calls compiled separately by GCC and Clang. Those checks
establish the tested behavior; they are not complete compiler conformance.

## Serialized identity

Both inspection formats include `translation_unit.compiler`, spelled `gcc` or
`clang`; binding reports also include `compiler`. Version 3 is the declaration-only
inspection shape, and version 5 includes checked code. The compiler field was
additive in versions 1/2; versions 3/4 also require the new VLA identity field,
as described in the [inspection migration](inspection.md#migration-from-versions-1-and-2).
Consumers should check the shape version and tolerate unknown fields. Archived
results without compiler identity use their recorded target's default.

Profiles also own a [C11 or GNU11 language mode](language-modes.md). GNU11 is the
default. The mode is independent of ABI and compiler family, and appears in
inspection and binding reports as `language_mode`.
