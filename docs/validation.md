# Validation

## Native corpus

The [upstream corpus](../corpus/README.md) builds pinned releases of zlib, SQLite, zstd,
and libgit2 and processes their untouched public headers. Native runs on x86_64 and
AArch64 Linux and macOS passed 5,444 C/Rust comparisons per target and actual FFI
calls into all four libraries.

The same runs matched 1,284 function signatures and three global types with bindgen
and independently checked every complete generated record against C. Depending on
the target, that covered 109–111 records and 628–638 ordinary field offsets. The
comparison gate passed with no unexplained differences; exact API equivalence
remains false, with each accepted difference recorded and justified.

The [recorded evidence](../corpus/evidence/native-06cefbe/summary.json) identifies
the tested commits and configurations. See [compatibility](compatibility.md)
for coverage and gaps. [Benchmarks](benchmarks.md) and [fuzzing](../fuzz/README.md)
record separate performance and malformed-input checks.

## Conformance

The [conformance guide](conformance.md) describes the scope of language,
preprocessor, ABI, and consumer checks. The external C suite now exercises both
compiler-preprocessed input and original source through Toucan's preprocessor;
both routes pass the [Rust 1.96 CI gate](../corpus/evidence/native-conformance-ci-2026-09-09/README.md).

## Platform and integration checks

The [ARMv7 test corrections](../corpus/evidence/armv7-test-matrices-2026-09-09/README.md)
pass all seven CI workflows at `6c66ec7`, including full native suites with
ignored tests enabled on Linux x86-64 and AArch64, and all-features tests and
package checks on Linux and Windows.

The [combined Builder validation](../corpus/evidence/native-callbacks-2026-09-09/README.md)
checks the combined Builder and analysis changes: 1,268 workspace tests and
Rustdoc pass on Linux; recorded GitHub jobs also pass on Linux x64/ARM and Windows.
The [Apple Silicon validation at `ac3312d`](../corpus/evidence/macos-arm64-ac3312d-2026-09-09/README.md)
passes workspace and package checks, the native corpus, all four zstd profiles,
and a SQLite consumer. It does not validate later commits or full uv/ty builds.
The macOS workflow remains opt-in on pull requests; this run allocated no Intel
runners.

More recent bounded native checks include [installed Windows ARM64 SDK headers](../corpus/evidence/windows-arm64-sdk-2026-09-09/README.md)
with generated Rust layouts and an [i686 zstd C/Rust consumer](../corpus/evidence/i686-zstd-consumer-2026-09-09/README.md)
with byte-identical compression outputs. They establish those recorded paths;
they do not validate every project configuration or the latest macOS source.
