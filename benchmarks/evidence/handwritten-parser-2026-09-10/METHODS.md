# Handwritten parser validation, 2026-09-10

The handwritten parser generates exactly the same Rust bindings and normalized
Builder reports for zlib 1.3.1, SQLite 3.45.1, zstd 1.5.7, and libgit2 1.9.1 as
`f1e8dc89acc74675cf81475fdc3402a76a3d09e9`. Checked semantic values also match;
source ranges have the explicit differences below. Paired timing and allocation measurements passed. Correctness-preflight elapsed
times are excluded from benchmark results.

## Output checks

The preflight covers 14 workloads and four complete Builder reports:

| Workload | Output comparison |
| --- | --- |
| Four Builder headers | Exact Rust bytes and reports, excluding report timings |
| Five direct parser inputs | Every AST value and visitor count matches; 1,670 ranges omit trailing whitespace |
| Five retained semantic compilations | Every non-range value matches; 17,133 ranges omit trailing whitespace and 19 restore a missing closing attribute delimiter |

The fifth input is untouched zlib `adler32.c`, with five function definitions,
229 statements, and 666 expressions. Direct parser inputs are frozen Clang 18
preprocessing output; both variants read identical bytes. Checked and Builder
workloads include Toucan's own preprocessing and semantic normalization.

Exact parser-span and checked-output equality are **false**. The comparator
retains every changed range. A whitespace change requires an unchanged start,
a shorter end, and an exclusively ASCII-whitespace suffix in the original UTF-8
source bytes. Checked ranges must also be contiguous, non-synthetic source
ranges at the explicitly reviewed occurrence, scope, or noreturn locations.
All other JSON fields, including source text and dependencies, stay exact.

The 19 delimiter restorations match [an explicit manifest](checked-span-corrections.json)
by workload, source SHA-256, JSON path, old and new range, occurrence kind,
restored byte `0x29`, and complete resulting spelling. These fix a pre-existing
normalization bug: moving `) )` to `)) ` without updating the source map could
omit the original final delimiter. Any unlisted non-whitespace change fails.
There is no general exception for differing source locations.

Two failed strict preflights are preserved. The first exposed 200 omitted
attribute delimiters in the initial handwritten candidate. Removing text
normalization fixed those and restored the 19 delimiters that the baseline
already omitted. Independent struct, union, and enum attribute probes confirm
original operand locations while preserving semantic types and layouts.

## Build and measurement

The exact baseline and candidate harness sources are identical. Both use Rust's
System allocator, release optimization, fat LTO, one codegen unit, no incremental
compilation, and Ohm's experimental Cargo defaults disabled. The 32 registry
dependencies retain the same versions and checksums. Normal and allocation
counter binaries are frozen separately and identified by SHA-256.

The normal binary supplies timing results. A separate allocator wrapper counts
allocation and reallocation calls and cumulative requested bytes. It delegates
unchanged pointers, layouts, and sizes to System. Requested bytes are allocation
traffic, not live heap; whole-process RSS also includes output serialization.

Each process discards a warmup, then makes repeated calls. Parser and semantic
timers include the owned-input API and result construction. Snapshotting,
verification, and result destruction happen after the timer. Builder timings
include Rust rendering. Workload and variant order are randomized; every timing
round runs both variants. Every measured output must match that variant's own
frozen, approved preflight hash. Qualification flags follow the measurements.

Run measurements only after local native builds, compiler probes, and sanitizer
runs have finished. The capture uses seven process pairs and five calls per workload for timing
(196 processes, 980 calls); allocation counters use five pairs and three calls
(140 processes, 420 calls).
CPU affinity does not control activity elsewhere on the shared virtual machine.

## Artifacts and replay

- `summary.json`: binary identity, workload counts, and qualifications.
- `measurement-summary.json`: all paired latency, allocation, and RSS results, including observed pair ranges.
- `timings.json.gz` and `allocations.json.gz`: complete process observations and frozen output hashes.
- `measurement-environment.json`: CPU, OS, allocator/build qualification, and source verification.
- `measurement-artifacts.tar.gz` and `measurement-artifacts.json`: every raw process file, stored as SHA-256 content blobs with a path-to-blob manifest. Identical output snapshots are stored once. Every blob and original-file mapping was verified before removing duplicate temporary files; both uncompressed capture.json files remain available on the original host.
- `adversarial-review.json`: final disposition of the independent review findings.
- `preflight.json.gz`: all successful preflight comparisons and range proofs.
- `baseline-source.json.gz`, `candidate-source.json.gz`: source and executable hashes.
- `input-manifest.json.gz`: all 151 physical input/request hashes.
- `capture-artifacts.tar.gz`: exact harness source and locks, original comparison
  and build helpers, requests, preprocessed inputs, complete baseline/candidate
  outputs, failed strict comparisons, build logs, and independent review evidence.
- `capture-artifacts.json`: each archived file's SHA-256 and length.
- `build_variant.py` and `harness/`: a build helper accepting explicit checkout,
  output, Cargo cache, target, and shared-build paths. It refuses to overwrite an
  existing output directory and rejects source changes during either build.

For example, build each checkout into its own target directory:

```sh
python3 build_variant.py \
  --source /path/to/toucan-checkout \
  --output /path/to/frozen-variant \
  --target-dir /path/to/variant-target \
  --build-dir /path/to/shared-ohm-build \
  --cargo-home /path/to/cargo-cache
```

The archived runners retain original paths as provenance. Extract them into a
fresh directory, recreate the recorded project/system headers, and update paths
when replaying elsewhere. Freeze a new baseline preflight and input inventory
before comparing a new candidate; preserve the original evidence separately.
The source-pinned delimiter exception manifest deliberately rejects different
source bytes and requires a fresh review if inputs change. The two standalone
comparison validation scripts exercise 34 acceptance/rejection checks, including
UTF-8 byte offsets, quoted fake span text, changed semantic fields, fragmented
sources, and every correction-manifest key.

## Interpreting results

Reported changes are medians of the candidate/baseline ratios within each paired
round. Displayed baseline and candidate milliseconds are separate medians of
process medians, so their quotient can differ from the reported paired ratio.
Observed pair ranges are minimum/maximum ratios, not confidence intervals. Every
latency pair improved in this capture. RSS reductions are smaller and noisier;
RSS includes snapshot serialization and retained process state outside the timer.

Allocation medians improved for all workloads. Fourteen of fifteen baseline
SQLite checked calls recorded 46,395 allocation requests and 22,718,384 requested
bytes; one recorded one additional request and 848 additional bytes. The sample
is retained. All other recorded allocation counts and byte totals were constant
within each workload/variant. Counter timings do not contribute to latency
claims. Native/compiler/fuzzer work and compression were stopped before timing
and resumed only after both captures completed.
