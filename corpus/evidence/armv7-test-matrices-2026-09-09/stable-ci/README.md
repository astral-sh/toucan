# PR344 stable CI record

All seven automatic workflows passed for head `6c66ec7266165c1be28e372edb2bfdccee252a01`, observed at 2026-09-09 20:47:28 UTC. Fifteen jobs executed successfully; the scheduled fuzz campaign was skipped. No macOS workflow ran.

Every executed job's full checkout log names merge commit `127f3384e896d7cc0ab54c28ac35ff4e06af4cc0`. Its tree and the PR head tree are both `5add1daf6a75aa3435c2c94381f4eb6578996208`. The frozen local-source parity report independently records equality of all 630 source blobs with the successful local run. The API's PR base SHA and the synthetic merge's first parent differ; the recorded tree equality establishes which source was tested.

## Test results

These totals sum result lines within each exact logged command group. They exclude adjacent commands and are not a deduplicated count of unique test cases.

| Command | Runner | Passed | Failed | Ignored |
| --- | --- | ---: | ---: | ---: |
| Full workspace, including ignored tests | Linux x86_64 | 1,363 | 0 | 0 |
| Full workspace, including ignored tests | Linux AArch64 | 1,360 | 0 | 0 |
| Workspace, all features | Linux x86_64 | 1,093 | 0 | 263 |
| Workspace, all features | Windows x86_64 | 1,086 | 0 | 251 |
| Separate parameter-entry oracle | Linux x86_64 | 9 | 0 | 0 |
| Separate parameter-entry oracle | Linux AArch64 | 9 | 0 | 0 |
| Generated bindings and consumer, Rust 1.64 | Linux x86_64 | 10 | 0 | 0 |
| Generated bindings and consumer, Rust 1.64 | Linux AArch64 | 10 | 0 | 0 |

Both `cargo package --workspace --all-features --locked` steps passed. Package verification compiles packages and emits no test-result lines; those steps are recorded separately from workspace tests.

The CI test/package/lint jobs used stable Rust 1.98.1 (`48a229cea`, 2026-09-01). The frontend MSRV job checked the workspace with Rust 1.96.1. Native corpus and C acceptance jobs built with Rust 1.96.0, and generated-code compatibility used Rust 1.64.0. Fuzz smoke used nightly 1.100.0. Exact observed version lines are retained per job; this stable CI proof is separate from the local Ohm run.

## Retained evidence and verification

`summary.json` contains schema version 1, exact workflow/job identities, all final step statuses, source identity, toolchain observations, and ten command-scoped result streams. `logs/` holds all 15 unabridged job logs as deterministic gzip files. `api/` contains the final GitHub run/job responses, merge/PR identity and artifact metadata. `manifest.json` hashes every other file. Each compressed payload also records its original size and SHA-256.

Run offline with Python 3.10 or newer:

```sh
python3 verify.py
```

The verifier checks file inventory, compressed and decompressed digests, checkout identity, tree equality, all job outcomes, exact command boundaries and result counts. It does not re-run builds. The separate source-inventory hash identifies the local source evidence in the enclosing supplement.

Artifact ZIP files were not downloaded for this supplement; retained artifact digests are GitHub metadata, not independent content verification. This record does not establish a new uv/ty application run or a sustained fuzz campaign. Those historical proofs remain separate.
