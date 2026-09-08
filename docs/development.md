# Development

## Scope

Build an integrated C frontend for preprocessing, parsing, declaration semantics,
constant evaluation, and target layout. Rust bindings are the first consumer; API
compatibility checks and indexing should use the same semantic representation.
C++ and machine-code generation are subsequent projects.

Reusable libraries must not run compiler subprocesses, select a global allocator,
change process state, or hide unsupported ABI semantics. The parser and layout
engine are local, attributed forks of `lang-c` and `repc`, respectively. Both are
licensed under MIT or Apache-2.0; the separate GPL-licensed `cly` tool is not used.

## Review stack

Each layer builds on the preceding PR. Summaries state the behavior added and the
validation actually run. The stack separates workspace/source infrastructure,
target layout, preprocessing, declaration analysis, bindings, CLI integration,
real-header validation, adversarial testing, and performance measurement.

## First integration milestone

Generate bindings from untouched, versioned zlib, SQLite, zstd, and libgit2 headers.
On x86-64 and AArch64 Linux and macOS, compare constants, record sizes, alignments,
and field offsets with independently C-compiled probes. Compile generated Rust and
exercise actual library calls, callbacks, and aggregate arguments and returns.
Every test records header versions, target, compiler, and relevant defines.

Examples of useful operations are zlib and zstd compression roundtrips, SQLite
prepare/bind/step/finalize, and libgit2 OID conversion and repository operations.
Function-pointer sentinels and bitfields need dedicated tests; size and alignment
alone do not prove calling-convention compatibility.

## Production acceptance

- Preprocessing: macro rescanning, argument prescan, suppression, stringification,
  pasting, variadics, conditionals, include search, source provenance, and limits.
- Semantics: scoped names, compatible redeclarations, complete/incomplete types,
  integer promotions/conversions, constant expressions, qualifiers, and diagnostics.
- ABI: explicit target models, packing and alignment, bitfields, calling conventions,
  flexible arrays, and diagnosed unsupported extensions.
- Safety: malformed-input regression tests, bounded resource use, fuzz campaigns,
  dependency review, and independent review of generated unsafe interfaces.
- Performance: reproducible phase timings and peak memory on the same pinned corpus,
  compared with bindgen using equivalent output scope. Publish regressions too.
- Distribution: Linux/macOS CI, target/sysroot documentation, stable public API design,
  versioned output, and reproducible release artifacts.

Tests added to the repository are not evidence that all these gates have passed.
Document measured results and remaining gaps separately.


## Package verification

Run `cargo package --workspace --locked` before publishing. Workspace dependencies
carry both a local path and a release version so the generated manifests can resolve
outside this checkout. CI packages every crate and tests `--all-features` on Linux,
macOS, and Windows, including the CLI's jemalloc or mimalloc configuration. Compiler
oracle tests run separately on the native Linux and macOS jobs with
`--include-ignored`.

## Compiler oracles

Use `toucan_test_support::compiler_acceptance` when comparing a compiler result
with expected acceptance. It returns `Ok(true)` for acceptance, `Ok(false)` for
an ordinary diagnostic, and `Err` for a compiler failure. Compare with
`Ok(expected)` so a crashing compiler cannot satisfy a negative source test.
Keep assertions for the expected diagnostic as well. The helper recognizes
crashes reported by compiler drivers through exit code 1, including the Apple
Clang frontend failure that exposed this gap.

This contract applies to GCC, Clang and rustc invocations. Generated executables,
Rust test harnesses and CLI validation have separate exit contracts. The shared
helper has no dependencies and is used through versioned dev-dependencies;
normal package verification includes it.

The [compiler-oracle audit](../corpus/evidence/compiler-oracles-2026-09-08.json)
records the migrated call sites, crash regression, cross-target compilation and
package checks.
