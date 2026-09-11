# Benchmarks

## CodSpeed regression benchmarks

The `toucan_benchmark` crate follows
[Ruff's benchmark setup](https://github.com/astral-sh/ruff/tree/a02a6b88216021f67381b4eb296f81d054b8e225/crates/ruff_benchmark).
The [Benchmarks workflow](../.github/workflows/benchmarks.yml) runs CodSpeed CPU
simulation and memory profiling on relevant pull requests, pushes to `main`, and
manual dispatches. It uses the existing `profiling` Cargo profile and OIDC
authentication; no `CODSPEED_TOKEN` secret is needed. The repository must be
enabled in the [CodSpeed GitHub integration](https://codspeed.io/docs/integrations/ci/github-actions).

The four `bindings` benchmarks generate bindings through `toucan_bindgen::Builder`
for the pinned zlib, SQLite, zstd, and libgit2 public headers in
`corpus/manifest.json`. Each iteration clones the configured Builder, reads and
preprocesses headers, parses and analyzes declarations, and generates a Rust
string. Comments and default derives are enabled; rustfmt and layout tests are
disabled. File allowlists select the reached project headers and their dependencies,
including the headers behind libgit2's umbrella. An untimed generation rejects
skipped declarations and checks a known public entry point before each benchmark.

Corpus download/build, configuration setup, and the initial generation happen
outside measurement. These track Toucan regressions; the bindgen comparisons
below remain separate workloads. CPU simulation measures computational work,
not filesystem latency or end-to-end consumer build time.

To run locally on Linux x86-64, install Clang 18, CMake, Tcl, Python 3.12+, and the
GitHub CLI, then prepare the corpus and run ordinary Criterion benchmarks:

```console
python3 scripts/prepare_corpus.py
SOURCE_DATE_EPOCH=0 cargo bench -p toucan_benchmark --bench bindings
```

Set `TOUCAN_BENCH_CORPUS` to reuse another `prepared.json`, or
`TOUCAN_BENCH_CLANG_INCLUDE` to override `/usr/lib/llvm-18/lib/clang/18/include`.
Relative corpus paths are resolved from the repository root.
The target is fixed to `x86_64-unknown-linux-gnu` with the host `/` sysroot.
Missing projects or metadata that differs from the pinned manifest fail the run.
To smoke-test each workload once, append `-- --test` to `cargo bench`.

To check the same instrumented build used in CI:

```console
cargo install cargo-codspeed --version 5.0.1 --locked
cargo codspeed build -m simulation -m memory --features codspeed --profile profiling -p toucan_benchmark --bench bindings --locked
SOURCE_DATE_EPOCH=0 cargo codspeed run
```

Local CodSpeed runs check that the benchmarks execute; the GitHub action collects
and uploads the performance measurements.

## Builder API comparison

The September 10, 2026 refresh compares the Builder APIs at Toucan
[`5b6dfb9`](https://github.com/astral-sh/toucan/commit/5b6dfb9dcc7b52f0f1b98d34902a5ab21f67fbc9)
and bindgen 0.72.1 with libclang 18.1.3. Both use identical public-header roots
and output policies, including comments and default derives. Bindgen's default
include-path discovery is enabled. The generators share one uninstrumented
release executable built with Ohm Rust 1.98.1-dev (`ohm-1.98.1-1`), experimental
Cargo defaults disabled, and the system allocator.

| Project | Toucan `5b6dfb9` | bindgen 0.72.1 | Median paired speedup |
| --- | ---: | ---: | ---: |
| zlib 1.3.1 | 17.24 ms | 122.04 ms | 7.05× |
| SQLite 3.45.1 | 17.03 ms | 161.75 ms | 9.52× |
| zstd 1.5.7 | 6.84 ms | 106.66 ms | 15.54× |
| libgit2 1.9.1 | 152.78 ms | 265.29 ms | 1.76× |

Times are medians of seven process medians; speedups are medians of seven matched
pair ratios, which need not equal the ratios of the displayed times. Engine
order is randomized with seed `20260910`. Each process records its first call
separately and measures ten subsequent calls. The two reported configurations
contribute 560 measured generations and 56 first calls on CPU 3 of a shared
AMD EPYC-Milan Linux host. All output, input, binary, libclang, and source hashes
pass. The [refresh capture](https://github.com/astral-sh/toucan/pull/536) retains raw
samples, paired ranges, first-call costs, and toolchain identities.

The fresh preflight validates shared signatures, constants, layouts, and native
FFI calls. All eight FFI programs pass. Structural API equality holds for zlib
and zstd. SQLite's returned callback, ten extra libgit2 aliases, libgit2's private
bitfield storage, and three signed sentinel values remain documented differences.
Generated modules request Rust 1.64 output and were compiled with the current
Ohm compiler; Rust 1.64 itself was not rerun for this refresh.
These timings measure generation only on the named Linux workloads. They do
not establish application build speedups, performance on other headers, or a
statistically significant improvement between revisions. The
[previous capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/benchmarks/evidence/builder-66c8739/README.md)
measured `66c87396` before the handwritten parser and later frontend improvements.

## Subprocess harness

`scripts/benchmark.py` measures subprocess startup and binding generation against
a supplied header. It records every sample's iteration, tool order, output hash,
stderr, size, time, and Linux peak resident memory. The JSON also records tool
versions, commands, CPU model, allowed CPU affinity where available, target, and
emitted function names. One warmup is discarded; measured tool order is shuffled
with a fixed seed. Each command has a 60-second timeout, configurable with `--timeout`.

```console
cargo build --release -p toucan_cli
python3 scripts/benchmark.py /path/to/sqlite3.h \
  --target x86_64-unknown-linux-gnu --sysroot / \
  --allowlist 'sqlite3*' --allowlist 'SQLITE*' --bindgen /path/to/bindgen \
  --iterations 15 --output benchmark-results/sqlite.json
```

Use the same target resource headers with `-I` for both tools. Bindgen runs without
doc comments, layout tests, or rustfmt, with enum name prefixes disabled and signed
macro constants by default, matching the API comparison configuration. Toucan emits compile-time
layout assertions. Matching function names is a useful scope check, but does not
prove identical types, signatures, or complete output equivalence.

The harness measures warm filesystem-cache operation and includes output capture.
An untimed Toucan report discovers header dependencies before the warmup. The
harness hashes those files and both executables before and after measurement; it
saves the results and exits with an error if they changed. These hashes do not
cover headers used only by bindgen or shared libraries. Output hashes reveal
whether a tool produced different output between samples.

Avoid concurrent builds and other heavy workloads while collecting results. The
harness records affinity but does not pin CPUs or control CPU frequency or host
load. Use an external affinity tool when needed and retain that configuration.
It does not measure allocation counts or peak memory on non-Linux hosts. Report
regressions and workload differences, and retain raw JSON when publishing numbers.
Production performance claims require a larger corpus and equivalent-output review.

To compare the default CLI allocator with the system allocator, build a separate
binary with `--no-default-features` and record that configuration. Library consumers
choose their own process allocator.


## Keeping results

Use CI artifacts or ignored `benchmark-results` output for raw samples, generated
files, and manifests. Published numbers should link a preserved result and name
its source revision, configuration, and comparison limits. See
[artifact policy](validation.md#results-and-artifacts).
