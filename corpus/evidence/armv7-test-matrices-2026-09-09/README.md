# ARMv7 test expectations: local and stable CI validation

The corrected target expectations passed 1,093 local tests, with no failures and
263 ignored tests. Three selected ignored compiler oracle tests also passed.
All seven updated CI workflows also passed. The local Ohm run and the stable CI
results below retain their separate source and toolchain records.

## Source and scope

The tests ran in a worktree based on
[`7146565`](https://github.com/astral-sh/toucan/commit/7146565cbfe11241416cd9e53ed142448c705fe7)
plus the captured 20-file patch (SHA-256
`16513d2265eef8f4932bbf9f93176779d153ffa144dd83d526b6231ce8898bcf`).
The 630 frontend source/configuration files were unchanged before and after the
full run. All 630 file hashes and the README separately match Git commit
[`6c66ec7`](https://github.com/astral-sh/toucan/commit/6c66ec7266165c1be28e372edb2bfdccee252a01).
The Git match identifies the final inputs; the local commands ran on the
uncommitted worktree, not on that Git commit.

The patch changes tests and the README. The only changed Rust file outside test
directories, `crates/toucan_semantic/src/checked/statement.rs`, has an identical
production prefix before its `#[cfg(test)]` module. This corrects test assumptions
about ARMv7, including unsupported `__int128`, compiler defaults and the Rust 1.78
binding requirement. It does not change production behavior or weaken that
minimum-version guard. The source inventory and before/after bytes for all 20
changed files are retained; unchanged frontend files are represented by hashes.

## Results

| Evidence | Recorded result |
| --- | --- |
| Original PR #344 CI at `39a4c0c` | Six workflows passed; CI failed. Both full-test jobs reported the same 21 failing tests across 19 Cargo targets. |
| Original upstream corpus at `39a4c0c` | Passed on x86-64 and AArch64, including the Rust 1.64 string-macro step. |
| Direct Clang 18.1.3 probes | 30 invocations with expected acceptance or rejection; authored inputs, commands and streams retained. |
| Final local workspace | 1,093 passed, 0 failed, 263 ignored across 296 test/doc-test result groups. |
| Selected ignored oracles | Sync signatures, minimum vector width, and overflow operations: one pass each. |
| Independent patch review | Passed; 20 final file hashes match the reviewed patch. |
| Historical ARMv7 layout evidence | Earlier QEMU run at `8c8e74c`: 52 command records and 104 streams rechecked, including the Clang layout probe and 16 C static assertions. |

The full local command was:

```console
cargo +ohm test --locked --workspace --all-features --no-fail-fast
```

It used Ohm `1.98.1-1`; Cargo/rustc versions, build directories and environment
are recorded in the captured local manifest. The three selected tests used
`cargo +ohm test --locked --all-features -p toucan_semantic --test <module>
<test> -- --ignored`. Earlier focused Rust 1.64 logs are retained with their
original provenance; they are not a claim that the final full workspace ran all
ignored tests or used Rust 1.64 to build Toucan.

The direct probes preserve negative coverage for unsupported ARMv7 and i686
`__int128`, while accepting it on x86-64 and AArch64. Other probes establish the
specific compiler expectations used by this patch. The historical layout
package is an unchanged copy of earlier evidence, not a new execution. It uses
Toucan's Clang profile and QEMU; it does not establish GNU frontend support or
native ARM hardware execution.

## Stable CI

All seven automatic workflows passed at Git head `6c66ec7`. Fifteen jobs
executed successfully; the scheduled fuzz campaign was skipped. Every executed
job checked out merge commit `127f3384e896d7cc0ab54c28ac35ff4e06af4cc0`.
Its tree and the head tree are both `5add1daf6a75aa3435c2c94381f4eb6578996208`.
The 630 source hashes match the completed local run.

| Workflow | Result |
| --- | --- |
| [CI](https://github.com/astral-sh/toucan/actions/runs/34400650781) | Passed |
| [Upstream corpus](https://github.com/astral-sh/toucan/actions/runs/34400650865) | Passed |
| [C acceptance](https://github.com/astral-sh/toucan/actions/runs/34400650879) | Passed |
| [Csmith](https://github.com/astral-sh/toucan/actions/runs/34400650835) | Passed |
| [Fuzz tests](https://github.com/astral-sh/toucan/actions/runs/34400650883) | Smoke passed; campaign skipped |
| [Windows DLL consumers](https://github.com/astral-sh/toucan/actions/runs/34400650801) | Passed |
| [Musl ABI and consumers](https://github.com/astral-sh/toucan/actions/runs/34400650783) | Passed |

The full workspace command with `--include-ignored` passed 1,363 tests on Linux
x86-64 and 1,360 on Linux AArch64, with zero failures or ignored tests. These are
sums of result lines within that command, not unique test-case counts across
jobs. The separate parameter-entry command passed nine tests on each runner.
Both corpus jobs passed the Rust 1.64 compatibility step: four `rust_target`
tests and six `string_macros` tests each. Both package verification steps passed.
The separate all-feature workspace tests passed 1,093 tests on Linux (263
ignored) and 1,086 on Windows (251 ignored), with zero failures.

Test, package and lint jobs used stable Rust 1.98.1; the frontend MSRV job used
Rust 1.96.1. Corpus and C acceptance jobs built with Rust 1.96.0, generated-code
compatibility used Rust 1.64.0, and fuzz smoke used nightly 1.100.0. Exact version
lines and command boundaries are retained in [stable-ci/](stable-ci/README.md).
No macOS workflow ran in this batch. This record does not establish a new uv/ty
application run or a sustained fuzz campaign. Artifact ZIPs and binaries were
not downloaded; their retained GitHub digests are metadata, not independently
verified artifact contents.

## Files and verification

- `original-ci/` preserves the failed CI record unchanged, including logs,
  workflow metadata, checkout identity and all 21 failure details.
- `local-capture.json.gz` contains the original local reports, logs, authored
  probes, patch and changed source snapshots. Each file points to UTF-8 contents
  keyed by SHA-256; `origin` retains its original path or Git reference. Repeated
  contents are stored once, without truncation or text rewriting.
- `historical-armv7-layout/` preserves the earlier layout package unchanged.
- `stable-ci/` retains all 15 complete CI job logs, final API metadata, ten
  command result streams and its independently usable verifier.
- `local-independent-review.json.gz` preserves the independent local package
  audit; `final-independent-review.json` checks the combined package and CI
  addition. Both passed without findings. `verifier-controls.json` records three
  altered-copy rejection checks.
- `git-source-verification.json`, `local-source-parity.json` and
  `merge-summary.json` record source identity separately from test outcomes.
- `assembly-verification.json` records the checks performed while assembling
  this package. `manifest.json` hashes every packaged file except itself.

Run `python3 verify.py` with Python 3.10 or newer from any directory. It verifies
captured hashes and
streams, recomputes local test totals and original failure sets, checks source
stability and test-only changes, and cross-checks compiler exits and diagnostics.
It also invokes the standalone CI verifier to check job outcomes, checkout
identity, full log hashes, exact command boundaries and all recorded test totals.
It needs no compiler, repository checkout, network access or optional Python
package. It validates recorded evidence; it does not rerun the tests or fetch the
630 unchanged Git blobs. No executable binaries, active Cargo manifests,
symlinks, or downloaded upstream source/header/preprocessed bytes are included.
