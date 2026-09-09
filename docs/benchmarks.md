# Benchmarks

## Builder API comparison

The [September 9 callback Builder capture](../benchmarks/evidence/builder-callback-final/README.md)
compares the actual Builder APIs on identical public-header roots and output
policies, including comments and default derives. It measures validated and
published `7db2b850`, including nullable function typedefs and generation work limits:

| Project | Toucan Builder | bindgen | Median paired speedup |
| --- | ---: | ---: | ---: |
| zlib 1.3.1 | 22.05 ms | 118.03 ms | 5.34× |
| SQLite 3.45.1 | 26.07 ms | 156.25 ms | 5.98× |
| zstd 1.5.7 | 9.02 ms | 104.85 ms | 11.62× |
| libgit2 1.9.1 | 194.20 ms | 263.32 ms | 1.36× |

Times are medians of seven process medians; speedups are medians of seven matched
pair ratios. Each process records its first call separately and measures ten
subsequent calls, for 560 measured generations on CPU 3 of a shared Linux host.
All output, input, binary, libclang and source hashes pass. The evidence retains
raw samples, pair ranges, first-call costs, commands and toolchain identities.

The [preflight](../benchmarks/evidence/builder-preflight-callbacks/README.md)
validates shared signatures, constants, layouts and native FFI calls. Structural
API equality holds for zlib and zstd. SQLite's returned callback, ten extra
libgit2 aliases and three signed sentinel values remain documented differences.
These timings measure generation only. The [earlier selection capture](../benchmarks/evidence/builder-selection/README.md)
and [initial Builder capture](../benchmarks/evidence/builder-acfb815/README.md)
remain separate evidence; these shared-host runs do not establish a statistically
significant revision change. The older core-route results below use a different workload.

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

To compare the optional CLI allocator, build with `--features performance-allocator`
and record it as a separate binary/configuration. Library consumers choose their
own process allocator.

## Recorded Linux results

On 2026-09-08, commit `bb6a401` generated bindings for the four pinned public-header
workloads faster than bindgen 0.72.1 on an AMD EPYC-Milan Linux host. Both tools used
the full corpus allowlists, including SQLite declarations and uppercase constants.

Each result is the median of 15 subprocess runs after one warmup. Tool order was
shuffled within each iteration, with CPU affinity restricted to CPU 0 and warm
filesystem caches. Toucan used a release build and the system allocator. RSS is
the median of each subprocess's maximum resident memory.

| Project | Toucan | bindgen | Speedup | Toucan RSS | bindgen RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| [libgit2 1.9.1](../benchmarks/evidence/2026-09-08-bb6a401/libgit2.json) | 382.10 ms | 439.14 ms | 1.15× | 18.00 MiB | 100.65 MiB |
| [sqlite 3.45.1](../benchmarks/evidence/2026-09-08-bb6a401/sqlite.json) | 170.03 ms | 320.38 ms | 1.88× | 9.44 MiB | 83.20 MiB |
| [zlib 1.3.1](../benchmarks/evidence/2026-09-08-bb6a401/zlib.json) | 47.89 ms | 283.89 ms | 5.93× | 8.25 MiB | 79.50 MiB |
| [zstd 1.5.7](../benchmarks/evidence/2026-09-08-bb6a401/zstd.json) | 16.98 ms | 265.56 ms | 15.64× | 6.20 MiB | 77.66 MiB |

The [evidence summary](../benchmarks/evidence/2026-09-08-bb6a401/summary.json) records
binary, source, header, and output hashes, with all raw observations retained.
The measured output hashes match the independently verified bindings. The same
Toucan binary passed 5,444 C/Rust comparisons and real calls into all four built
libraries. Separate C probes checked all 111 complete records and 636 ordinary
field offsets.

The bindgen comparison found matching signatures for 1,284 functions and matching
types for three globals and 203 shared typedefs. There are no unexplained
differences; exact Rust API equality remains false. The reports retain seven
macro-type differences, three unsigned sentinel differences, SQLite's `xDlSym`
callback discrepancy, extra Toucan constants, and differences in helper names and
private bitfield storage. Independent C probes support the accepted type, value,
and callback differences.

Both generators' outputs remain byte-identical to the
[previous measured revision](../benchmarks/evidence/2026-09-08-1d8f508/summary.json).
These measurements include complex and binary128 types, vector intrinsics, and
the build-script adapter. They precede BMI target options, explicit language modes,
and function-contract changes. The same revision completed
[2,547,850 ASan fuzz executions](../fuzz/evidence/2026-09-08-bb6a401/summary.json)
without findings and passed the full workspace suite including native tests.

Independent builds used CPUs 4–15 and correctness probes used CPUs 16–19 during
timing. This shared host did not isolate memory bandwidth, filesystem activity,
or CPU frequency. All samples are retained. The earlier parser-limit regression
remains recorded in the [paired measurements](parser-limits.md#measured-cost).
These results cover four workloads on one machine; they do not measure cold
starts, in-process reuse, other platforms or allocators, or complete source analysis.

## Repeated library calls

`benchmarks/inprocess` compares library calls after one discarded warmup in each
process. The timed operation includes configuration, preprocessing, parsing, and
Rust source generation. It excludes process startup and the first libclang load.
Both libraries use the same standalone release profile and system allocator.

```console
cargo build --release --locked --manifest-path benchmarks/inprocess/Cargo.toml
python3 scripts/benchmark_inprocess.py benchmark-results/sqlite.json \
  --binary benchmarks/inprocess/target/release/toucan-inprocess-benchmark \
  --output benchmark-results/sqlite-inprocess
```

The input is a saved `scripts/benchmark.py` result containing both engines and a
sysroot. The runner preserves its target, include directories, and allowlists.
It checks every recorded header hash before and after measurement, and requires
each engine's complete output to match its own reference hash. It saves generated
source and every timing sample. The output directory must be new. These checks do
not establish exact API equality between the two generators or inventory headers
used only by libclang.

At library commit `0342e7f`, three randomized process pairs per header each ran
five measured calls after warmup, yielding 15 samples per engine and header. The
shared Linux x86-64 host was pinned to CPU 3; CPU frequency and memory bandwidth
were not isolated. The [raw evidence](../benchmarks/evidence/inprocess-0342e7f/summary.json)
includes the source manifest, driver build log, and full generated outputs.

| Project | Toucan | bindgen 0.72.1 | bindgen / Toucan |
| --- | ---: | ---: | ---: |
| zlib | 37.84 ms | 122.68 ms | 3.24× |
| SQLite | 191.13 ms | 165.19 ms | 0.86× |
| zstd | 9.08 ms | 107.92 ms | 11.88× |
| libgit2 | 376.02 ms | 262.36 ms | 0.70× |

Toucan is slower for SQLite and libgit2 in this embedding workload. The earlier
[subprocess results](#recorded-linux-results) include startup and should not be
used to describe repeated library calls. An independent
[earlier library baseline at `019012e`](../benchmarks/evidence/inprocess-019012e/summary.json)
shows the same pattern. Both runs preserve the complete output hashes from the
existing correctness artifacts; the later implementation added language and
compiler features without changing these four binding outputs.

### Avoiding declaration copies for literal arithmetic

The next [paired measurement](../benchmarks/evidence/literal-evaluation-2026-09-08/summary.json)
compares the frozen library baseline with literal-only constant evaluation using
a fresh, small environment. Queries involving identifiers, types, calls, or
compound literals keep the existing full environment. Both paths retain public
unit validation and target-specific arithmetic. A bounded AST traversal decides
which environment to use; it does not introduce a second expression evaluator.

| Project | Before | After | bindgen 0.72.1 | Before / after |
| --- | ---: | ---: | ---: | ---: |
| zlib | 38.94 ms | 31.12 ms | 132.46 ms | 1.25× |
| SQLite | 199.98 ms | 67.25 ms | 199.44 ms | 2.97× |
| zstd | 9.32 ms | 8.62 ms | 123.21 ms | 1.08× |
| libgit2 | 402.97 ms | 285.32 ms | 286.11 ms | 1.41× |

SQLite is about three times faster than bindgen in this run; libgit2 is roughly
equal. Each result again contains 15 measured calls after warmup, now with the
before, after, and bindgen process order randomized within each pair. CPU 3 ran
the benchmark; independent verification used other CPUs on the shared host.

Binding-generation allocations fall from 1,804,101 to 70,280 for SQLite and from
2,078,204 to 894,534 for libgit2. Configuration and parse allocations are unchanged.
All four complete outputs and report fields match except elapsed timings.
Another 341 macro expressions match across 22 compiler/target/language settings,
including their skipped-macro diagnostics. The recorded checks include native
constant probes, public-environment validation, query-local enum isolation, and
[34,955 address-sanitized binding executions](../fuzz/evidence/literal-evaluation-2026-09-08/README.md).
These measurements retain the same shared-host limitations as the baseline.

### Avoiding declaration copies for enumerator expressions

The [enumerator query capture](../benchmarks/evidence/constant-query-2026-09-08/README.md)
extends the bounded proof to expressions using known integer constants. Each
query copies only the values it references into a fresh analyzer. Type-dependent
and unknown expressions retain the complete environment and existing validation.

| Project | Before | After | bindgen 0.72.1 |
| --- | ---: | ---: | ---: |
| zlib | 33.210 ms | 33.507 ms | 118.930 ms |
| SQLite | 51.625 ms | 52.034 ms | 154.641 ms |
| zstd | 8.838 ms | 8.886 ms | 100.890 ms |
| libgit2 | 285.329 ms | 218.466 ms | 259.455 ms |

Libgit2 generation is 23.4% faster than the `a215c7e` baseline in this capture.
Its binding phase makes 75.8% fewer allocation requests and requests 70.7% fewer
bytes. All four outputs and binding reports match before and after, apart from
elapsed times. Another 164,400 query results match across all 88 supported
compiler, target, and language settings, including types, diagnostics, and offsets.

Seven randomized rounds contain 420 timed library calls on CPU 30, using the
system allocator and separately built source trees. Allocation counters and
Callgrind profiles were captured separately. These are warm library calls on a
shared host; process startup and first libclang initialization are excluded.
The other projects' medians are 0.5–0.9% higher, so this capture does not establish
unchanged latency for those routes. The artifact preserves raw samples, complete
outputs, all 149 actual header hashes, source, and reproduction drivers.

## Allocator comparison

A paired run at commit `f78baa8` compared the system allocator with the CLI's
`performance-allocator` feature (jemalloc on Linux). Both release binaries used
Rust 1.98.1 and the same source. The four projects and two binaries were shuffled
within each of 15 measured iterations, after one discarded warmup, on CPU 0.

| Header | System median | jemalloc median | Speedup | System / jemalloc peak RSS |
| --- | ---: | ---: | ---: | ---: |
| libgit2 | 361.83 ms | 238.81 ms | 1.52× | 16.50 / 17.75 MiB |
| SQLite | 152.07 ms | 91.24 ms | 1.67× | 7.65 / 9.00 MiB |
| zlib | 43.39 ms | 37.49 ms | 1.16× | 6.75 / 7.50 MiB |
| zstd | 14.36 ms | 14.31 ms | 1.00× | 4.91 / 6.25 MiB |

Every output was byte-identical across the two allocators and all samples. They
also match the output hashes in the published `4ac7511` benchmark evidence (and the current measurements).
Source, binaries, and header dependencies were verified; the
[raw observations and reports](../benchmarks/evidence/2026-09-08-allocators-f78baa8/evidence.json)
and [exact capture script](../benchmarks/evidence/2026-09-08-allocators-f78baa8/runner.py)
are retained. The capture script records the original workspace paths.

These results support offering jemalloc for the larger header workloads, with a
small increase in memory. The default remains the system allocator; library users
choose their process allocator. This was a shared host with concurrent verification
work, warm caches, and uncontrolled CPU frequency. These measurements predate the
later atomic/MMX and parser changes and do not measure macOS or Windows allocators.

## Compiler query integration

The [combined query revision](../benchmarks/evidence/compiler-queries-2026-09-08/evidence.json)
compares the integrated C90, Microsoft declaration, allocation, and query layers
with the optimized literal-evaluation revision `daa1c0f`. Three randomized process
triples each measure five library calls after a discarded warmup. All complete
outputs and pinned header dependencies match their references. The system
allocator is used; other processes and CPU frequency on this shared host are
uncontrolled. These measurements are not isolated attribution to query handling.

| Header | Previous Toucan | Combined Toucan | bindgen 0.72.1 |
| --- | ---: | ---: | ---: |
| zlib | 31.40 ms | 33.90 ms | 130.38 ms |
| SQLite | 50.84 ms | 51.86 ms | 165.50 ms |
| zstd | 8.50 ms | 8.95 ms | 116.69 ms |
| libgit2 | 278.83 ms | 295.25 ms | 280.41 ms |

The combined revision is 2–8% slower than the earlier Toucan revision in this
run. Libgit2 is about 5% slower than bindgen; the other three headers remain
faster. Raw samples and the capture script are archived alongside the summary.
