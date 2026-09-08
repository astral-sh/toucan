# Binding derives: 2026-09-08

This capture checks the requested Copy, Debug, Default, PartialEq, and Eq options
against bindgen 0.72.1 and emitted Rust storage. It is a type-generation check,
not a completed AWS-LC consumer build or a performance measurement.

## Results

- Six trait configurations use the same 23-type C input and pinned bindgen binary.
  Differences are confined to zero/flexible-array storage traits, omission of
  equality for `MaybeUninit` bitfield padding, and omission of a zeroing Default
  for a struct containing a nonzero Rust enum. The latter avoids an invalid Rust
  value; the reference's unsafe default is never executed.
- Focused tests pass: four ordinary tests and three native tests. The native tests
  compile and run the generated storage on current Rust and actual Rust 1.64.0.
  They include four GCC/Clang C round trips, positive trait bounds, and expected
  compile failures for unavailable traits. Workspace ordinary tests and workspace
  all-target/all-feature Clippy also passed before this capture.
- The untouched aws-lc-sys 0.44.0 wrapper yields 295 selected type names from its
  thirty allowed headers. Its 73 generated layout tests pass on both Rust
  compilers; construction and drop of all 73 generated Default types also pass.
  No AWS-LC operation or unsafe union-member read is performed. Function and
  callback selection remain separate integration work.
- Fresh, separate baseline/candidate binaries emit identical Rust source and
  complete reports with only timings removed for zlib, SQLite, zstd, and libgit2.
  The baseline is the selective-enum layer plus its CLI correction.
- An AddressSanitizer run executes 3,815 inputs in 61.2 seconds, peaks at 515 MiB,
  and produces no artifacts. Seed coverage includes all 1,408 combinations of
  eleven profiles, eight language modes, and sixteen trait selectors. Source
  hashes are unchanged across the run. This bounded campaign is additional
  coverage, not a proof of absence of defects.

The callback ABI probes distinguish compiler acceptance from generator failure:
Clang accepts all three target inputs, while bindgen panics for the Windows
SysV-ABI input. That tool failure is not counted as a C rejection.

## Reproduction and provenance

`captures.tar.gz` contains exact probe inputs, commands, outputs, native-test logs,
AWS-LC type output and validation, the report drivers and dependency locks,
default-output reports, and the ASan runner/log/seed selector table.
`manifest.json` records the source and artifact hashes. Machine-local absolute
paths identify the captured sysroot/header inputs; adapt those paths when
replaying elsewhere. The immutable ASan binary remains in the local evidence
cache under `binding-derive-evidence/asan/bindings` and its hash is recorded in
`asan/summary.json`.

Focused checks from the candidate source:

```sh
cargo test -p toucan_bindings --test derives
cargo test -p toucan_bindings --test derives -- --ignored
TOUCAN_TEST_RUST_TOOLCHAIN=1.64.0 cargo test -p toucan_bindings --test derives -- --ignored
```

For trait comparison, run `reference/probe_traits.py` with bindgen 0.72.1 and the
recorded libclang, then run the candidate driver's `trait-cases` binary against
`reference/traits.h`. The ASan runner preserves each original seed byte and adds
only a padding comment to select profile, mode, and traits.
