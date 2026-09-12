# Full AST arena evaluation

This harness compares the default parser and complete frontend pipelines against
an immutable earlier revision. It uses the five inputs from the integrated parser
benchmarks: GCC-preprocessed zlib, SQLite, zstd, and libgit2 public headers, plus
zlib's unchanged `adler32.c`. The parser comparison also includes lang-c 0.15.1.

`build.py` builds identical harness source against each checkout. Its `full-arena`
feature selects the new visitor signature for output capture; the source revision
determines the parser implementation. Capture validates:

- Identical printed syntax trees across baseline, arena, and lang-c.
- Identical concrete spans for every span-bearing visitor hook across the two
  Toucan revisions, preserving traversal order.
- Identical serialized semantic results, separately for normal and retained code.
- Identical generated bindings and generation reports, excluding report timings.

Every input must pass capture before timing starts. Inputs remain unchanged;
unsupported syntax or changed outputs fail the run.

## Build and run

Prepare the pinned corpus using `scripts/prepare_corpus.py`, or reuse an existing
`prepared.json`. Supply an immutable baseline checkout and the completed arena
checkout. This experiment uses baseline commit
`13922786782de88565d051403bd497d12291673c`, which includes the parser benchmarks.

```sh
python3 benchmarks/arena/build.py \
  --source /path/to/baseline --revision BASELINE_SHA \
  --output benchmark-results/arena-baseline \
  --target-dir /path/to/baseline-target --cargo 'cargo +ohm'
python3 benchmarks/arena/build.py \
  --source /path/to/arena --revision ARENA_SHA --full-arena \
  --output benchmark-results/arena-head \
  --target-dir /path/to/arena-target --cargo 'cargo +ohm'

SOURCE_DATE_EPOCH=0 TOUCAN_BENCH_CORPUS=/path/to/prepared.json \
  TOUCAN_BENCH_PARSER_INPUTS=/absolute/path/to/frozen-inputs \
  benchmark-results/arena-baseline/timing prepare

SOURCE_DATE_EPOCH=0 TOUCAN_BENCH_CORPUS=/path/to/prepared.json \
  python3 benchmarks/arena/run.py \
  --baseline benchmark-results/arena-baseline \
  --head benchmark-results/arena-head \
  --inputs /absolute/path/to/frozen-inputs \
  --output benchmark-results/arena-comparison --cpu 3
```

Use `--cargo cargo` for a normal toolchain build. Local Ohm users should follow
the repository guidance, including a separate shared `CARGO_BUILD_BUILD_DIR` and
the local trust settings. Each checkout needs a distinct target directory. Both
builds must use the same compiler and registry dependency versions. Dependencies
must already be cached; builds use `--offline` and a pinned lockfile.

The scripts require Linux, Python 3.11+, `taskset`, and `/usr/bin/time`. The chosen
CPU must be in the process's allowed affinity. Existing output directories are
rejected to preserve earlier captures. Stop other local builds before timing.

## Measurement boundaries

The runner randomizes participant order within seven process pairs by default.
Each process warms its selected operation once, then records 20 iterations on a
reused 16 MiB frontend stack. Worker creation and input-file reading are excluded
from operation and destruction timings. Both parser engines run on that stack.

| Stage | Timed operation | Untimed setup |
| --- | --- | --- |
| Parser | Tokenization and fresh AST construction with default limits | Clone the frozen preprocessed input |
| Analysis, normal/retained | Public `analyze_with_options`, including its internal parsing and checking | Read the frozen source and configure retention |
| Builder | Clone the configured Builder, read/preprocess headers, parse/check, generate a Rust string | Load corpus metadata and configure the Builder |

Result destruction is timed separately. Each sample also records the sum of its
operation and destruction timings, so parser-only and parse-and-drop comparisons
use the same runs. Semantic measurements include parsing because the public API
accepts source text. Internal AST destruction during semantic checking is part
of that operation. Builder captures reject skipped declarations and check a known
entry point before measured runs.

Allocation counting uses a separate build that wraps the system allocator.
Allocation/reallocation counts and requested bytes cover the operation, excluding
setup. Peak live requested bytes and live result bytes include the setup input
clone; `setup_live_bytes` records its contribution separately. Every measurement
checks that destroying the result returns live bytes to their pre-setup value.
These counters exclude allocator metadata, stacks, and mapped pages.

RSS uses fresh, uninstrumented processes and `/usr/bin/time`, with three samples
per participant. Each process performs one operation without a warmup or capture.
Peak RSS includes process initialization, input loading, configuration, stack
pages, and result destruction. It is reported separately from requested heap
bytes and from warm operation timing.

## Evidence

Build reports record source and harness SHA-256 hashes before compilation, verify
that those files remain unchanged afterward, and retain compiler versions, build
commands, lockfiles, selected environment settings, and binary hashes. The runner
retains exact input manifests, full captures, raw paired samples, phase medians,
paired ratios, allocation counters, RSS samples, and Builder dependency hashes.
Input, dependency, and binary hashes are checked again after measurement.

Keep outputs in ignored `benchmark-results` directories or CI artifacts. Report
both improvements and regressions, the revision pair, compiler/configuration,
workloads, and process ranges. Shared-host measurements do not by themselves
establish statistical significance or performance on other C inputs.
