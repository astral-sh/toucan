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

On 2026-09-08, commit `4945364` generated bindings for the four pinned public-header workloads
faster than bindgen 0.72.1 on an AMD EPYC-Milan Linux host. Both tools used the full
allowlists from the corpus manifest. SQLite includes both `sqlite3*` declarations
and `SQLITE*` constants.

Each result is the median of 15 subprocess runs after one warmup, with the two tools
shuffled within each iteration and the process restricted to CPU 0. Filesystem
caches were warm. Toucan used a release build and the system allocator. RSS is the
median of each subprocess's maximum resident memory.

| Project | Toucan | bindgen | Speedup | Toucan RSS | bindgen RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| [libgit2 1.9.1](../benchmarks/evidence/2026-09-08-4945364/libgit2.json) | 394.64 ms | 464.33 ms | 1.18× | 17.75 MiB | 100.66 MiB |
| [SQLite 3.45.1](../benchmarks/evidence/2026-09-08-4945364/sqlite.json) | 173.75 ms | 330.77 ms | 1.90× | 8.93 MiB | 83.20 MiB |
| [zlib 1.3.1](../benchmarks/evidence/2026-09-08-4945364/zlib.json) | 48.70 ms | 295.86 ms | 6.08× | 8.00 MiB | 79.75 MiB |
| [zstd 1.5.7](../benchmarks/evidence/2026-09-08-4945364/zstd.json) | 16.90 ms | 279.00 ms | 16.51× | 5.95 MiB | 77.91 MiB |

The [evidence summary](../benchmarks/evidence/2026-09-08-4945364/summary.json) records binary,
source, header, and output hashes; each project links to its unchanged raw samples.
The measured output hashes match the outputs used for correctness verification.
The same Toucan executable passed 5,444 C/Rust comparisons and real calls into all
four built libraries. This includes separate checks for the C expression types and
Rust enum representations of 679 enumerators. A second C probe compared all 111
complete records and 636 ordinary field offsets.

The bindgen comparison found matching signatures for 1,284 functions and matching
types for three globals and 203 shared typedefs. It passed with no unexplained
differences, but exact API equivalence remains false. The reports retain seven macro-type differences, three
unsigned sentinel differences, SQLite's `xDlSym` callback discrepancy, extra Toucan
constants, and differences in helper names and private bitfield storage. Independent
C probes support the accepted type, value, and callback differences.

Compared with [the previous measured revision](../benchmarks/evidence/2026-09-08-4ac7511/summary.json),
Toucan’s medians increased by 12–17% as checking and parser resource limits expanded.
This comparison includes intervening changes and shared-host variation; the
[paired parser-limit measurements](parser-limits.md#measured-cost) isolate that layer.
The raw samples retain outliers from the shared host; CPU affinity did not isolate
memory bandwidth, filesystem activity, or frequency changes.

These measurements cover one machine and four workloads. They do not measure cold
starts, in-process reuse, other platforms or allocators, or a complete C frontend.

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
