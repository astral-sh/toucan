# Development

## Workspace and checks

See [architecture](architecture.md) for the crate inventory and component
boundaries. Run the workspace checks from the repository root:

```console
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

C compilers are used by validation tools as an independent reference. See
[validation](validation.md) for recorded results and their scope.

## Scope

Build an integrated C frontend for preprocessing, parsing, declaration semantics,
constant evaluation, and target layout. Rust bindings are the first consumer; API
compatibility checks and indexing should use the same semantic representation.
C++ and machine-code generation are subsequent projects.

Reusable libraries must not run compiler subprocesses, select a global allocator,
change process state, or hide unsupported ABI semantics. The parser and layout
engine are local, attributed forks of `lang-c` and `repc`, respectively. Both are
licensed under MIT or Apache-2.0; the separate GPL-licensed `cly` tool is not used.

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

## Binary releases

We use [cargo-dist](https://axodotdev.github.io/cargo-dist/) 0.32.0 to build
GitHub Releases. The workflow has local pull-request path filters because this
cargo-dist version cannot configure them. `allow-dirty = ["ci"]` preserves these
edits and disables cargo-dist's generated-workflow freshness check.

After changing `dist-workspace.toml` or `crates/toucan_cli/dist.toml`, install that
version of `dist`, temporarily remove `allow-dirty`, and regenerate:

```console
dist generate
dist plan
dist build --target x86_64-unknown-linux-gnu
```

Restore the pull-request path filters and `allow-dirty` afterward, and review the
workflow diff. Use your host target for the local build. The
[`Release` workflow](../.github/workflows/release.yml) builds all six targets and
uploads archives, checksums, and installers when a PR changes source, build
configuration, or packaged README/license files. Changes confined to guides,
consumer fixtures, scripts, or fuzz inputs skip this release matrix. PRs do not
publish a release. macOS builds use Apple Silicon runners for both architectures;
Windows ARM64 is cross-compiled on an x86-64 Windows runner. Linux builds use
Ubuntu 22.04 on each architecture.

To publish, first update the workspace version, local dependency versions, and
lockfile together and merge the change. Run `Release` from `main` in GitHub
Actions with `tag` set to that version, such as `v0.0.1`. Use `dry-run` to build
and upload artifacts without publishing. After successful builds, the workflow
creates the tag and GitHub Release. Tags with a prerelease suffix produce a
prerelease. Pushing a tag alone does not start this workflow.

Only the `toucan` executable is distributed. The parser's debugging executable
and the experimental AWS-LC `bindgen` adapter are excluded. This workflow does
not publish crates to crates.io; package publication is a separate step.

## macOS CI

Pull requests run the native suites and corpus on both Linux architectures, plus
Linux and Windows package checks. Each PR in a stack has a distinct ref, so
per-ref concurrency does not prevent duplicate macOS work across the stack.

The separate `macOS validation` workflow runs Apple Silicon tests, packages, and
the corpus on relevant pushes to `main`. Add the `run-macos` label to an
integration PR to validate that revision. To
request another revision, remove and reapply the label. It can also be dispatched
against a selected branch.

Intel Macs are an opt-in compatibility target. Use the `run-macos-intel` label or
enable the dispatch's `intel` input to include them. Routine PR and `main` runs do
not allocate Intel runners. An explicit Intel run checks its native calling
conventions, Apple SDK and runtime, and actual C/Rust consumer calls.

Only one macOS validation request runs across the repository; a newer request
cancels the older one. Tests precede the corpus so even an Intel request occupies
at most one Intel runner at a time. Native tests have a 60-minute limit, packages
30 minutes, and the corpus 40 minutes. Failed tests prevent the corpus run.

## Additional platform validation

Run these workflows manually from GitHub Actions, selecting the branch to check:

| Workflow | Coverage |
| --- | --- |
| [ARMv7 GNU hard-float acceptance](../.github/workflows/armv7-native.yml) | GCC/Clang layouts and generated C/Rust calls under QEMU. |
| [i686 GNU native acceptance](../.github/workflows/i686-native.yml) | Native 32-bit C/Rust layouts and calls. |
| [i686 zstd Rust consumer](../.github/workflows/i686-zstd-consumer.yml) | Generated bindings consumed by a native 32-bit zstd application. |
| [Windows ARM64 native ABI acceptance](../.github/workflows/windows-arm64.yml) | MSVC/LLVM and native C/Rust calls. |
| [Windows ARM64 SDK headers](../.github/workflows/windows-arm64-sdk.yml) | Installed SDK headers and generated Rust layouts on ARM64. |
| [Windows x64 SDK headers](../.github/workflows/windows-x64-sdk.yml) | Installed SDK headers and generated Rust layouts on x64. |

These checks are opt-in and retain command logs and results as workflow artifacts.
They do not run automatically on pull requests or pushes to `main`.

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


## Generated-C conformance

The [Csmith audit](../corpus/conformance/csmith/README.md) checks pinned or supplied
programs against strict GCC and Clang acceptance, both preprocessed frontend
profiles, and normal/retained declaration parity. Its Linux CI gate uses four
pinned programs and native CRC/UBSan controls. Larger runs retain every exclusion
and tool failure separately; a compiler-rejected program never counts as a
frontend success.

## Reports and regression fixtures

Keep generated reports, logs, source manifests, and benchmark captures in ignored
output directories or CI artifacts. Commit small reusable fixtures and the tests
that exercise them. Link release-level results to the tested commit and workflow
run; see [validation](validation.md#results-and-artifacts).
