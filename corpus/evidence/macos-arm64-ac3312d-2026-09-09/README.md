# Apple Silicon validation at ac3312d

[GitHub Actions run 34384065196](https://github.com/astral-sh/toucan/actions/runs/34384065196) passed on September 9, 2026 at **`ac3312d204258db2d2f50cf41e1e56ccefc9ec22`**. This package preserves that commit's evidence; it does not validate subsequent Windows/ARM changes or later integration commits.

All three executed jobs used **macos-15-arm64** runners. No Intel runner job was present. The Ubuntu-only lint and Rust 1.96 jobs were skipped; the native corpus itself built and ran with Rust 1.96.0.

## Results

| Stage | Observed result |
| --- | --- |
| Workspace tests, including ignored tests | 1,346 reported passing harness executions; 0 failed, 0 ignored |
| Workspace tests with all features | 1,082 reported passing harness executions; 0 failed, 257 ignored |
| Native parameter-entry compiler oracles | 9 tests passed |
| Package verification with all features | 11 workspace packages passed |
| Native C/Rust corpus | 4 libraries; 5,444 comparisons; FFI executables passed |
| C/Rust complete-record probes | 109 records, 628 field offsets, 846 comparisons passed |
| API comparison with bindgen | 1,284 common function signatures equal; 262 accepted differences; 0 unexpected differences |
| Rust 1.64 generated bindings | 109 layout tests and 628 field-offset assertions passed |
| Real zstd consumer | Four feature profiles passed on Rust 1.96 and again on Rust 1.64 |
| zstd's unchanged binding build script | Four feature profiles passed through Toucan's Builder API |
| Real SQLite consumer | rusqlite 0.40.2 / libsqlite3-sys 0.38.2 passed; 1,551 independent C comparisons |

The two workspace test counts sum test-harness result lines from each step. They include nested worker/generated-test executions and must not be presented as counts of unique tests.

The native corpus used zlib 1.3.1, SQLite 3.45.1, zstd 1.5.7, and libgit2 1.9.1. The separate rusqlite consumer used SQLite 3.53.2 and exercised prepared parameters, scalar/trace/extension callbacks, and database serialization/restoration.

The zstd consumer used zstd 0.13.3, zstd-safe 7.2.4, and zstd-sys 2.0.16+zstd.1.5.7. Profiles were `default`, `experimental`, `zstdmt`, and `experimental-zstdmt`. Tests covered bulk and streaming compression, trained dictionaries, experimental magicless/COVER paths, threaded compression, and the shared thread pool where enabled. Across direct binding substitution (Rust 1.96 and 1.64) and the unchanged build script, **75 pairs of runtime output files** match byte-for-byte and have verified SHA-256 hashes.

## Scope and qualifications

**Generated APIs are not identical.** The comparison artifact explicitly records `exact_equivalence: false`. The 262 accepted differences include additional C-checked Toucan constants, helper aliases/record representations, C-checked constant types, bitfield helper storage, and SQLite's `xDlSym` callback signature. Full details and reasons remain in the artifact.

The macOS complete-C-translation-unit step was skipped. This run does not establish a full uv or ty build, native Intel macOS behavior, later-commit correctness, or macOS benchmark results. Passing this finite corpus is not proof for every C program or ABI.

## Preserved files

- `artifact.zip`: Original GitHub artifact, unchanged. SHA-256 `3baf1599f7d4ee677af47e92271c9f57c0fa8b59b0f8bb0bbb3fbf7883121d2d` matches GitHub's reported digest.
- `logs.zip`: Complete run logs, unchanged; includes package, allocator, workspace, compiler-oracle, and corpus step output.
- `summary.json`: Stage outcomes, counts, source/library/binding hashes, tool identities, and limitations.
- `manifest.json`: SHA-256 and size of package files and every member of both archives.
- `github.json`: Captured run, job/step, and artifact metadata confirming the exact head.
- `source.json`: Tested Git commit/tree and SHA-256 hashes of its workflow, manifest, and validation-script inputs.
- `verification.json`: Locally verified runtime artifact pairs and hash relationships.

Extract `artifact.zip` to inspect the original `results/*/evidence.json`, generated Rust/C probes, compiler output, comparison inventories, consumer dep-info, and runtime artifacts. Absolute paths in those records describe the original runner. Tool/source hashes recorded by that runner are retained; executable binaries and complete sysroot/source trees are not part of this package.

The release frontend SHA-256 was `02f9c03a4a2fb2b7e010300e0e7760027e485fa84505dafd8ab6e4d5ad525a78`. Apple Clang was 17.0.0 (clang-1700.0.13.5); the native SDK came from Xcode 16.4. See `summary.json` for full identities and the distinct compiler versions used in workspace oracle tests.
