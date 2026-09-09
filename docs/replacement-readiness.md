# Replacement readiness

Toucan is experimental. We have proved specific pinned build-script paths, but
have not established a universal replacement for bindgen in Ruff, ty, uv, or
other consumers. The checks below identify their own tested revisions, including
the combined-source baseline `7db2b850`. The remaining gates apply to the
eventual release revision.

## Pinned consumer paths

| Path | Result on the frozen source | Boundary |
| --- | --- | --- |
| `ty` and `uv` through `zstd-sys` | [Unchanged build scripts and selected frontend artifacts](../corpus/evidence/astral-builder-7db2b85/summary.json); two ty vendored tests and 19 uv extraction tests pass, with matching CLI and wheel behavior. | Native x86-64 Linux zstd generation. This is not the full workspace suite. |
| `zstd-sys`, `zstd-safe`, and `zstd` on i686 | [Native ELF32 consumer comparison](../corpus/evidence/i686-zstd-consumer-2026-09-09/README.md) passes independent GCC/Clang layouts, four generated Rust layout tests, and byte-identical bulk, streaming, and dictionary artifacts. | Native i686 GNU Linux with default features at `7b9f483`; the unchanged build script consumes two replaced binding files. This does not validate full ty or uv builds. |
| `aws-lc-sys` and `aws-lc-rs` crypto | [Paired reference and Toucan builds](../corpus/evidence/aws-lc-builder-7db2b85/summary.json) pass 41 deterministic runtime comparisons, six C/Rust layouts, and generated layout tests. | Native x86-64 Linux crypto-only generation; optional profiles are tracked separately. |
| `aws-lc-sys` with `all-bindings` | [Paired build and consumption](../corpus/evidence/aws-lc-all-bindings-7db2b85/README.md) pass the 41 crypto artifacts, six C/Rust layouts, a memory-BIO call, and 98 generated layout tests per generator. The [frozen API differential](../corpus/evidence/aws-lc-all-bindings-differential-7db2b85/README.md) matches 2,618 functions and 60 globals after canonical ELF symbol matching. | Native x86-64 Linux; SSL and FIPS are disabled. The frozen output differs in an incomplete tag's public name, three explicit padding fields, and 13 extra aliases. A later [name correction](../corpus/evidence/incomplete-record-names/README.md) has no full consumer rebuild yet; the other differences remain. |
| `aws-lc-sys` with `ssl,all-bindings` | [Paired native SSL builds](../corpus/evidence/aws-lc-ssl-consumer-7db2b85-2026-09-09/README.md) pass 106 layout tests per generator, 41 matching crypto artifacts, six C/Rust layouts, and C and Rust SSL context/object calls. Canonical ELF comparison matches 3,230 functions and 60 globals. | Frozen `7db2b85` source on native x86-64 Linux. No full handshake, FIPS, external executable, or exact generated API equality; 15 extra aliases and four record shapes differ. |
| `aws-lc-sys` external `bindgen` executable | [Native build through the unchanged upstream script](../corpus/evidence/aws-lc-external-cli-317756d/README.md) passes 41 crypto artifacts, six C/Rust layouts, and 98 generated layout tests. The same-command reference agrees on all 2,618 functions and 3,851 constants. | Native Linux x86-64 crypto only. Bindgen-cli leaves 60 globals unprefixed despite `--prefix-link-name`; Toucan prefixes them and all 60 symbols exist in the native archive. The original AWS-LC manifest still compiles bindgen and clang-sys as build dependencies. |
| `aws-lc-fips-sys` external `bindgen` executable | [Native FIPS build through the unchanged upstream script](../corpus/evidence/aws-lc-external-fips-2026-09-09/README.md) invokes Toucan, consumes its generated Rust, and passes an actual integrity-check FFI call, 41 reference-matching crypto artifacts, six C/Rust layouts, and 97 generated layout tests. | Native Linux x86-64 only. FIPS certification and full API coverage are not established. Toucan fixes a C-validated unprefixed integrity symbol that bindgen-cli also links incorrectly. The upstream script still calls libclang. |
| `uv` HTTPS through AWS-LC | [Paired binaries](../corpus/evidence/uv-tls-builder-7db2b85/summary.json) install the same payload with TLS 1.2 and 1.3, and both reject an unrelated CA or wrong hostname before making a request. | Native x86-64 Linux, the recorded provider and selected features. |
| Four untouched public-header projects | [Fresh preflight at `66c87396`](../benchmarks/evidence/builder-66c8739/README.md#validation) checks zlib, SQLite, zstd, and libgit2 with native C probes, generated Rust, and actual FFI calls. All eight outputs match the earlier preflight byte for byte. | Zlib and zstd pass structural API equality. SQLite's corrected returned callback, ten extra libgit2 aliases, and three signed sentinels remain recorded differences. |

The [native validation](../corpus/evidence/native-callbacks-2026-09-09/README.md)
records 1,268 passing workspace tests, Rustdoc, three bounded AddressSanitizer
campaigns, and eight successful GitHub workflows. The enum analyzer at the same
source also matches seven pinned zstd/AWS enums and 100 C discriminants; see the
[schema-3 evidence](../corpus/evidence/binding-rustified-enums-schema3-2026-09-09.json.gz).
The [Builder benchmark at `66c87396`](../benchmarks/evidence/builder-66c8739/README.md)
measures generation on these four projects. It does not measure application build
time or establish speed on other headers. A separate [peak resident memory run](../benchmarks/evidence/builder-peak-rss-callbacks/README.md)
measures the earlier `7db2b850` binary in fresh Linux processes, including
loaded libraries; it does not measure the memory of a full application build.

## Gates for a general drop-in release

1. **Target coverage.** [`Target::parse`](../crates/toucan_target/src/lib.rs) accepts
   ten triples: x86-64 and AArch64 Linux with GNU or musl libc, i686 GNU Linux,
   ARMv7 GNU Linux with the Clang hard-float profile, both macOS architectures,
   and x86-64 and ARM64 Windows MSVC. The pinned
   [uv platform policy](https://github.com/astral-sh/uv/blob/d28a3ee3d0f7122b0da64b0226d2e173e7d23747/docs/reference/policies/platforms.md)
   also ships Linux PPC64LE, RISC-V64, and s390x, which remain unsupported. ARMv7
   has compiler layout and predefined-macro checks, plus [GCC/Clang C/Rust
   execution under QEMU](../corpus/evidence/armv7-qemu-2026-09-09/README.md) at
   O0/O2 with 256 rounds per compiler/optimization pair. Native ARM hardware
   and full uv/ty consumer builds remain unvalidated. ARMv7 output requires Rust 1.78 to guard its
   hard-float ABI. i686 has
   [cross-target C layout evidence](../corpus/evidence/i686-target-2026-09-09/README.md)
   and [native 32-bit C/Rust FFI evidence](../corpus/evidence/i686-native-af0d174-2026-09-09/README.md)
   for GCC and Clang at O0/O2 on Ubuntu 24.04, plus the pinned zstd consumer above.
   Full ty/uv i686 builds, arbitrary glibc headers, and other distribution
   environments still need checks.
   Enabling source generation unconditionally across those distributions would
   fail at target selection. Add and validate each required ABI and header
   environment before switching that distribution, or explicitly limit adoption
   to validated targets and configurations and retain the existing generator elsewhere.
2. **Current-source platform and consumer validation.** The published combined
   source passed Linux x64/ARM corpus and Windows packaging; the latest paired
   application checks ran on Linux x64. Earlier macOS C/FFI evidence covers an
   older source. Run Apple Silicon validation of the intended integration head
   and selected consumer builds before a macOS switch; request Intel separately
   if distributing generation on Intel Macs. Windows x86-64 DLL calls have
   bounded [native evidence](../corpus/evidence/windows-dll-native-f57e9fa/summary.json);
   [Windows ARM64 SDK C layouts and DLL calls](../corpus/evidence/windows-arm64-native-4ce552f/README.md)
   also pass. The [installed Windows ARM64 SDK run](../corpus/evidence/windows-arm64-sdk-2026-09-09/README.md)
   and [Windows x64 SDK run](../corpus/evidence/windows-x64-sdk-2026-09-09/README.md)
   pass all seven stages for `basetsd.h`, `winnt.h`, and `windows.h` with SDK
   `10.0.26100.0`, including native Rust layout checks. Each run selects six,
   four, and seven supported types respectively and reports zero skipped
   declarations. Full Windows consumers and other SDK versions remain open;
   selected x64 `__ptr32` bindings receive an explicit unsupported-ABI diagnostic.
   The first [Apple Silicon integration run](https://github.com/astral-sh/toucan/actions/runs/34378956751)
   failed on test fixture and cross-compiler assumptions, so it does not establish
   current-source macOS acceptance. The [macOS workflow](../.github/workflows/macos.yml)
   keeps Intel opt-in; that integration run allocated only Apple Silicon runners.
3. **Optional generator profiles.** The selected AWS crypto, `all-bindings`,
   SSL through the Builder adapter, standalone crypto, and standalone FIPS
   routes have separate native evidence. Unchanged `aws-lc-sys 0.44.0` refuses
   the [external SSL route](../corpus/evidence/aws-lc-external-fips-2026-09-09/README.md)
   before calling any generator; its upstream build script must change to enable
   that combination. The unmodified external FIPS script still calls
   `bindgen::clang_version()`; removing libclang from that installation also
   requires an upstream build-script change. Test each adopting release profile
   with its original build script, consumed generated bindings, independent
   layouts, and representative calls. Cover additional zstd feature/header
   combinations if selecting them.
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
