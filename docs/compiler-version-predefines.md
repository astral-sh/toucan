# Compiler version predefines

Toucan's GNU profiles define `__GNUC__`, `__GNUC_MINOR__`, and
`__GNUC_PATCHLEVEL__` as **13, 3, 0**. Clang profiles define their corresponding
`__clang_*` version markers as **18, 1, 3**. Non-Windows Clang profiles also
provide GNU compatibility version **4, 2, 1**. These deterministic values select
header branches; they do not identify an installed compiler or promise every
feature of the named release.

Capability queries, including `__has_builtin`, describe implemented features
independently. Raising a version marker does not enable optimizer settings,
optional instruction sets, or unimplemented builtin families. Unsupported source
constructs and Rust representations retain their diagnostics. In particular,
GNU `_Float64x` and `_Float128` scalar Rust bindings remain unsupported.

Windows omits `__STDC__`, matching Clang's Microsoft C profile in ISO and GNU
language modes. It retains `__STDC_HOSTED__` and the existing Microsoft ABI
markers. Other targets define `__STDC__` as `1`. Language-version and inline-mode
macros continue to follow the selected C mode.

Library callers can override individual markers through `Config.preprocessor.defines`;
the CLI accepts equivalent `-D` options. Source `#undef` directives remain
respected. Changing a macro does not change the compiler family stored in the
translation unit, its layout rules, or its feature-query catalog. Select that
family with `CompilerProfile` or `--compiler` and use matching resource headers.
In particular, overriding GNU's version to select legacy floating typedefs does
not permit redeclaring its `_FloatN` keywords. Such a header branch is diagnosed,
matching the same override in native GCC. Clang's ordinary `_Float32`, `_Float64`,
`_Float32x`, `_Float64x`, and `_Float128` aliases remain available.

## Validation

The version-marker and source-branch tests cover all eleven compiler profiles
and every supported language mode. The native marker fixture compares standard
and GNU compatibility markers with cross-target Clang without assuming that the
installed Clang's own version equals Toucan's compatibility profile.

The captured GCC 13.3 and Clang 18.1.3 probes include 72 empty-input macro tables
across eight native C modes. GNU AArch64 uses a cross compiler; these probes do
not execute AArch64 code. All Windows probes omit `__STDC__`.

Fresh preprocessing with the defaults generates bindings for untouched zlib,
SQLite, zstd, and libgit2 headers under both compiler families. Seven real
translation units from these libraries pass ordinary analysis and retained graph
validation under each family. Their commands retain project definitions and
include paths and contain no compiler-version overrides. The retained checks use
the source-audit budgets: two million preprocessor tokens and graph nodes, eight
million edges, and 128 MiB of owned payload.

The same fourteen compiler/project combinations also pass native syntax checking,
fresh native preprocessing, and ordinary/retained analysis of the resulting
compiler-produced source. Each pair agrees on its translation unit and graph
invariants; this does not require the native and Toucan preprocessed text to match.

The four musl libc ABI fixtures retain byte-identical generated output on both
architectures and compiler families. Generated `strtof32`, `strtof64`, `strtof32x`,
and `cosf32` bindings run against installed x86-64 glibc with current Rust and
Rust 1.64 at optimization levels 0 and 3. This establishes the tested calls and
header routes, not complete libc coverage or native execution on other targets.

The [validation record](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/compiler-version-predefines-2026-09-08.json.gz)
contains commands, compiler and frontend hashes, unchanged input hashes, graph
results, diagnostics, and the generated-call programs. Compiler definitions follow
[GCC's documented version markers](https://gcc.gnu.org/onlinedocs/cpp/Common-Predefined-Macros.html)
and [Clang's builtin macros](https://releases.llvm.org/18.1.8/tools/clang/docs/LanguageExtensions.html#builtin-macros).

The [combined validation](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/compiler-version-root-integration-2026-09-08/summary.json)
repeats the header and source routes after integrating all eight C modes and DLL
semantics. It includes 8,272 ordinary/retained seed pairs, native-preprocessed
controls, and the bounded ASan campaign with unchanged source hashes.
