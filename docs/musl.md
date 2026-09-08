# Musl targets

Toucan supports distinct `x86_64-unknown-linux-musl` and
`aarch64-unknown-linux-musl` targets with GCC or Clang semantics. Generated Rust
checks `target_env = "musl"`. The Builder selects these triples from Cargo's
`TARGET` and uses its Clang profile.

The targets use their architecture's Linux data model and calling conventions.
Both are LP64. AArch64 uses unsigned plain `char` and `wchar_t`, binary128
`long double`, and a 32-byte `va_list`. x86-64 uses signed plain `char` and
`wchar_t`, x87 `long double` in 16-byte storage, and a 24-byte `va_list`.
The frontend does not invent a `__MUSL__` compiler macro.

## Headers and linking

Supply a musl sysroot, including its libc headers. `--sysroot` searches
`usr/include/x86_64-linux-musl` or `usr/include/aarch64-linux-musl`, followed by
`usr/include`; the latter also supports sysroots without a multiarch directory.
Explicit include directories retain their specified order. Put musl headers
before compiler resource headers when specifying both with `-I`:

```console
toucan bindgen api.h --target x86_64-unknown-linux-musl --compiler clang \
  --sysroot /opt/musl-sysroot \
  -I /opt/musl-sysroot/usr/include/x86_64-linux-musl \
  -I /opt/clang/lib/clang/18/include -o bindings.rs
```

This keeps musl's `stddef.h` ahead of the compiler's copy while making resource
headers such as `stdatomic.h` available. The frontend runs on the build host;
it does not discover a cross compiler or linker. Configure the C compiler and
Rust linker separately when building the library and application.

## Validation

`scripts/verify_musl_abi.py` compiles the same fixture with independent C and Rust
compilers. It checks 49 sizes, alignments, offsets, and type properties, then runs
1,000 rounds of aggregate calls, callbacks, register and stack arguments,
variadics with `va_copy`, bitfield accessors, and atomic pointer accessors. C uses
`-O0` and `-O2`; Rust uses optimization levels 0 and 3. Supply `--rustc` repeatedly
to cover both the current compiler and actual Rust 1.64. Generated source targets
Rust 1.64 throughout.

The harness records compiler versions, commands, header and binary hashes, the
Rust toolchain's self-contained musl archive, and results. An explicit `--runner`
is required for a different CPU. The [musl workflow](../.github/workflows/musl.yml)
adds native x86-64 and AArch64 execution and runs all four unchanged zstd-sys
Builder consumer configurations. A workflow definition is not a passing native
run; saved evidence identifies its actual execution route.

The [saved local evidence](../corpus/evidence/musl-2026-09-08/summary.json) uses
untouched Ubuntu musl 1.2.4-2 headers. x86-64
executables run natively; AArch64 executables run through QEMU user emulation.
C/Rust ABI probes use GCC 13.3 and Clang 18.1, with current Rust and Rust 1.64.
The four public header projects also have independently compiled constant and
record-layout comparisons. These header probes do not call their libraries;
the separate zstd Builder runs exercise actual library calls and byte-for-byte
consumer artifacts.

Existing unsupported Rust call ABIs remain errors, including bitfield records,
vectors, `long double`, and atomic aggregates passed by value. Narrow Clang atomic
scalars also remain unsupported. These checks do not establish complete libc coverage or the rest of uv's release matrix.

## Native CI evidence

At commit `019012e`, the [native musl workflow](https://github.com/astral-sh/toucan/actions/runs/34246121203) passed on x86-64 and AArch64. The two architectures ran 32 C↔Rust executables across GCC, Clang, current Rust, Rust 1.64, and both optimization settings. Each executable checks 49 ABI properties and runs 1,000 call rounds.

The same jobs consumed eight generated zstd binding files across the four Builder configurations on each architecture. The [saved artifacts](../corpus/evidence/musl-native-019012e/summary.json) include the workflow metadata, file hashes, raw commands, generated source, and consumer comparisons. This closes the native AArch64 execution gap in the earlier local QEMU evidence. The result applies to the recorded revision and configurations.
