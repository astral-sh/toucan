# PR #344 validation at 39a4c0c

All seven monitored workflows completed. Six passed; CI failed in its full and
package test jobs.

| Workflow | Result |
| --- | --- |
| [Upstream corpus](https://github.com/astral-sh/toucan/actions/runs/34397339560) | Passed on x86-64 and AArch64, including Rust 1.64 |
| [CI](https://github.com/astral-sh/toucan/actions/runs/34397339479) | Failed: four test jobs; lint and Rust 1.96 passed |
| [C acceptance](https://github.com/astral-sh/toucan/actions/runs/34397339466) | Passed |
| [Csmith](https://github.com/astral-sh/toucan/actions/runs/34397339469) | Passed |
| [Fuzz](https://github.com/astral-sh/toucan/actions/runs/34397339455) | Smoke passed; campaign skipped |
| [Windows DLL](https://github.com/astral-sh/toucan/actions/runs/34397339528) | Passed |
| [Musl](https://github.com/astral-sh/toucan/actions/runs/34397339492) | Passed on both architectures |

## Tested revision

Every run reports head `39a4c0c0992ca943745268452a31a5d2f6079313`.
All seven captured job logs record checkout
`ad9b6d5a0c3256c81cb07f00dfb490d3e5a55976`. That PR merge commit has the same
Git tree as the head: `cd4f065277d1a2288c63d8aad0bdc3f7a8821394`.

## Rust 1.64 confirmation

Both corpus jobs ran:

```console
TOUCAN_TEST_RUST_TOOLCHAIN=1.64.0 cargo test --locked -p toucan --test rust_target --test string_macros -- --include-ignored
```

Each passed four Rust-target tests and six string-macro tests, with zero ignored
tests. The rest of each Rust 1.64 generated-layout and zstd-consumer step also
passed.

## CI failures

The full x86-64 and AArch64 test jobs reported the same 21 failed tests across
19 Cargo targets. The Ubuntu and Windows package jobs stopped at
`macro_queries_keep_object_alignment_and_size_t_integer_metadata`, which asked
for Rust 1.64 while generating ARMv7 bindings that require Rust 1.78.

Three ignored-test failures were additional to the existing local failure
inventory: the native oracle tests in `minimum_vector_width.rs`,
`overflow_builtins.rs`, and `sync_builtins.rs`. All three involved `__int128` on
ARMv7. The complete test names, locations, and diagnostics are preserved in
`failures.json.gz`. Follow-up changes require their own validation.

## Contents

`summary.json` contains final workflow/job results, revision identity, Rust 1.64
checks, and payload hashes. `workflow-metadata.json.gz` preserves the final
GitHub responses. `logs.json.gz` contains seven small job logs with their original
bytes; `summary.json` records each log's SHA-256. This record contains no downloaded
workflow artifacts. Workflow status polls used intervals of at least 60 seconds;
completed job logs were read as needed.
