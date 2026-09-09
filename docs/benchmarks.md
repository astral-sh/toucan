# Benchmarks

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
