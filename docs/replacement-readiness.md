# Replacement readiness

Toucan is experimental. We have proved specific pinned build-script paths, but
have not established a universal replacement for bindgen in Ruff, ty, uv, or
other consumers. This document ties the release decision to the tests actually
run on the combined source `7db2b850` and names the remaining gates.

## Pinned consumer paths

| Path | Result on the frozen source | Boundary |
| --- | --- | --- |
| `ty` and `uv` through `zstd-sys` | [Unchanged build scripts and selected frontend artifacts](../corpus/evidence/astral-builder-7db2b85/summary.json); two ty vendored tests and 19 uv extraction tests pass, with matching CLI and wheel behavior. | Native x86-64 Linux zstd generation. This is not the full workspace suite. |
| `aws-lc-sys` and `aws-lc-rs` crypto | [Paired reference and Toucan builds](../corpus/evidence/aws-lc-builder-7db2b85/summary.json) pass 41 deterministic runtime comparisons, six C/Rust layouts, and generated layout tests. | Native x86-64 Linux crypto-only generation; optional profiles are tracked separately. |
| `aws-lc-sys` with `all-bindings` | [Paired build and consumption](../corpus/evidence/aws-lc-all-bindings-7db2b85/README.md) pass the 41 crypto artifacts, six C/Rust layouts, a memory-BIO call, and 98 generated layout tests per generator. | Native x86-64 Linux; SSL and FIPS are disabled, and complete API equality is unproven. |
| `uv` HTTPS through AWS-LC | [Paired binaries](../corpus/evidence/uv-tls-builder-7db2b85/summary.json) install the same payload with TLS 1.2 and 1.3, and both reject an unrelated CA or wrong hostname before making a request. | Native x86-64 Linux, the recorded provider and selected features. |
| Four untouched public-header projects | [Combined-source preflight](../benchmarks/evidence/builder-preflight-callbacks/README.md) checks zlib, SQLite, zstd, and libgit2 with native C probes, generated Rust, and actual FFI calls. | Zlib and zstd pass structural API equality. SQLite's corrected returned callback, ten extra libgit2 aliases, and three signed sentinels remain recorded differences. |

The [native validation](../corpus/evidence/native-callbacks-2026-09-09/README.md)
records 1,268 passing workspace tests, Rustdoc, three bounded AddressSanitizer
campaigns, and eight successful GitHub workflows. The enum analyzer at the same
source also matches seven pinned zstd/AWS enums and 100 C discriminants; see the
[schema-3 evidence](../corpus/evidence/binding-rustified-enums-schema3-2026-09-09.json.gz).
The [Builder benchmark](../benchmarks/evidence/builder-callback-final/README.md)
measures generation on these four projects. It does not measure application build
time or establish speed on other headers. A separate [peak resident memory run](../benchmarks/evidence/builder-peak-rss-callbacks/README.md)
measures the same frozen binary and requests in fresh Linux processes, including
loaded libraries; it does not measure the memory of a full application build.

## Gates for a general drop-in release

1. **Target coverage.** [`Target::parse`](../crates/toucan_target/src/lib.rs) accepts
   eight 64-bit triples: x86-64 and AArch64 Linux with GNU or musl libc, both
   macOS architectures, and x86-64 and ARM64 Windows MSVC. The pinned
   [uv platform policy](https://github.com/astral-sh/uv/blob/d28a3ee3d0f7122b0da64b0226d2e173e7d23747/docs/reference/policies/platforms.md)
   also ships Linux ARMv7, i686, PPC64LE, RISC-V64, and s390x.
   Enabling source generation unconditionally across those distributions would
   fail at target selection. Add and validate each required ABI and header
   environment before switching that distribution, or explicitly limit adoption
   to the eight supported targets and retain the existing generator elsewhere.
2. **Current-source platform and consumer validation.** The published combined
   source passed Linux x64/ARM corpus and Windows packaging; the latest paired
   application checks ran on Linux x64. Earlier macOS C/FFI evidence covers an
   older source. Run Apple Silicon validation of the intended integration head
   and selected consumer builds before a macOS switch; request Intel separately
   if distributing generation on Intel Macs. Windows DLL calls have bounded
   [native evidence](../corpus/evidence/windows-dll-native-f57e9fa/summary.json),
   while Windows ARM64 DLL calls and both Windows SDK headers and full consumer
   builds need their own checks.
   The [macOS workflow](../.github/workflows/macos.yml) keeps Intel opt-in, and
   the latest GitHub audit records zero macOS allocations.
3. **Optional generator profiles.** The recorded AWS crypto and `all-bindings`
   routes do not exercise SSL, FIPS, or the external `bindgen` executable mode.
   Test each profile that a release will use with the original build script, consumed
   generated bindings, independent layouts, and representative calls. External
   mode launches the standalone executable and cannot be replaced solely through
   a Cargo dependency substitution. Cover additional zstd feature/header
   combinations if selecting them in the adopting project.
4. **Public contract and safety.** Define a supported Builder API, diagnostic
   behavior, target set, and versioned inspection format before a stable release.
   Keep rejecting unsupported [C and Rust ABI forms](compatibility.md#current-gaps)
   explicitly. Review generated unsafe interfaces and extend sustained malformed
   input testing; three five-minute AddressSanitizer runs do not prove complete
   memory safety. Structural comparison does not prove trait implementations,
   private storage semantics, or every downstream Rust compile requirement.

A limited rollout can use the pinned Linux paths above while preserving a fallback
for other targets and configurations. A general replacement claim requires the
gates above to pass on the actual release revision. Rust 1.96 is not a blocker for
the pinned Ruff and uv sources; both workspaces already require it. Toucan's
public API and JSON remain experimental while the release contract is unsettled.
