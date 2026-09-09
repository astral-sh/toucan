# Stable CI native-preprocessor acceptance

[Run 34394052399](https://github.com/astral-sh/toucan/actions/runs/34394052399) passed
both GCC and Clang jobs on September 9, 2026, for
[PR #340](https://github.com/astral-sh/toucan/pull/340).

| Profile | Sources | Pedantic-positive | Accepted through compiler-preprocessed input | Accepted through original source |
| --- | ---: | ---: | ---: | ---: |
| GCC 13.3 | 220 | 211 | 211/211 | 211/211 |
| Clang 18.1.3 | 220 | 211 | 211/211 | 211/211 |

Each route accepted 219/220 exploratory cases. The sole difference was
`00144.c`, which discards a pointer qualifier and falls outside the shared
pedantic-positive subset. All native preprocessing operations succeeded. Neither
job recorded a timeout, crash, tool failure, changed input, or strict-positive
difference. Both jobs also passed all 14 acceptance-gate tests.

## Build provenance

Both Ubuntu 24.04 jobs used stable Rust 1.96.0 and built the CLI and existing
library probe from source:

```console
cargo build --locked --release -p toucan_cli -p toucan --bin toucan --example audit_translation_unit --no-default-features
```

The checkout was the PR merge commit
`97dc7dc9d17bb348a7e7ecebae083c30e8974ef5`. Its Git tree
`3f5faf4b300c44260bd15be8f2d93bf20d95d662` exactly matches the PR head,
`ca8924413da6fc01e44192f548bef0a18050ffd0`. The recorded driver, probe source,
and corpus-manifest hashes match files in that tree. Both jobs recorded the same
CLI and probe executable hashes; `summary.json` preserves them.

`build-verification.json` records an independent review of the checkout,
toolchain, compilation, gate-test, and acceptance-step logs. The compressed
capture preserves those logs and the five relevant files from the tested Git
tree. GitHub run, job, artifact, and tested-merge metadata are included.

This supplies stable release-build CI evidence for the native route previously
audited with local Ohm builds. It does not modify the frozen local evidence.

## Artifact verification

The downloaded ZIP files matched GitHub's artifact sizes and SHA-256 digests.
The two compressed reports preserve the uploaded `evidence.json` bytes exactly.
Verification recomputed eligibility and checked both routes against all 211
pedantic-positive members. It checked 10,170 recorded commands, 20,340 command
streams, and 880 native request/response pairs, including exit-code, JSON-status,
and overall-status consistency. Both native configuration controls also passed.

All 440 original source copies matched the pinned upstream archive. For each
case, the recorded source hashes, before/after dependency hashes, final dependency
hashes, and native preprocess/analyze dependency sets agreed. GCC read 220 sources
and 45 filesystem headers; Clang read 220 sources and 53 filesystem headers.
Toucan's shipped predefines and built-in headers remained enabled. Compiler macro
differences are recorded, and compiler dumps were not supplied as replacements.

To verify the preserved supplement without downloading sources or running
compilers:

```console
python3 verify.py
```

`capture.json.gz` deduplicates the retained command streams, requests, per-case
results, CI logs, and Toucan-owned sources. Full upstream source files,
preprocessed `.i` files, and declaration bodies are excluded; their hashes and
sizes remain available. Recorded compiler diagnostics and origin metadata are
retained. `artifact-verification.json` records the original archive comparisons.

## Scope

This is positive C acceptance evidence for the pinned corpus on native x86-64
Linux. It does not establish invalid-input rejection conformance, identical
preprocessing tokens or conditional branches, runtime behavior, ABI equivalence,
or coverage of every C construct.

The CI artifacts do not contain executable bytes, compiler-artifact JSON, a full
post-build source inventory, or filesystem header snapshots. Build attribution
therefore rests on the successful checkout/build logs, exact Git-tree identity,
and recorded source/binary hashes. Header stability checks compare hashes
recorded by the CI runner; the reviewer did not rehash the runner's files
or reconstruct its executables.
