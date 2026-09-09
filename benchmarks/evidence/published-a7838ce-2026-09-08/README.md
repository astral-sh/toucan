# Published frontend performance, 2026-09-08

The published `a7838ce` frontend generates the same bytes as `a3256d6` for all four
project routes. It takes less time than bindgen 0.72.1 on zlib, SQLite, and zstd.
Libgit2 remains slower than bindgen. These measurements do not establish an
improvement over the previous Toucan build.

## Generation time

Median elapsed milliseconds per in-process generation:

| Header | Toucan `a3256d6` | Toucan `a7838ce` | bindgen 0.72.1 | Toucan change |
| --- | ---: | ---: | ---: | ---: |
| zlib 1.3.1 | 33.812 | 33.520 | 121.591 | -0.9% |
| SQLite 3.45.1 | 52.199 | 52.874 | 158.404 | +1.3% |
| zstd 1.5.7 | 8.807 | 8.873 | 101.999 | +0.8% |
| libgit2 1.9.1 | 291.127 | 293.425 | 262.537 | +0.8% |

The primary capture contains seven rounds, with independently randomized project
and engine order within each round. Each process discards one warmup, then makes
five timed library calls: 84 processes and 420 timed calls. All processes use CPU
30 of the shared AMD EPYC-Milan virtual machine. CPU frequency, memory bandwidth,
and other processes are uncontrolled. Small changes need confirmation on an
isolated machine before being treated as regressions or improvements.

An earlier complete capture is preserved too. Its median libgit2 times were
291.663 / 291.754 / 262.540 ms for baseline / candidate / bindgen. The primary
capture followed a fuller header inventory that included three libclang-only
resource headers. Both captures use the same binaries and produce the same
outputs. The raw records include individual samples, round order, sample ranges,
and descriptive bootstrap intervals over the seven paired round medians. Those
intervals do not account for the shared machine's uncontrolled conditions.

Times include configuration, preprocessing, parsing, semantic analysis, and Rust
source generation through the existing `benchmarks/inprocess` harness. They
exclude process startup, request decoding, and the first libclang initialization.
Each call rebuilds the frontend's state. No custom allocator or `LD_PRELOAD` is
used. This is a warm in-process library comparison, not command startup or a Rust
consumer build measurement.

## Allocation traffic

The separate counter delegates each Rust allocation to `std::alloc::System` and
records setup, `parse_file`, and binding generation. It counts allocation and
reallocation requests and sums the requested sizes, including the full new size
of each reallocation. These counts are allocation traffic, not live heap or peak
RSS. Instrumented runs do not contribute to timing results. Native C++ allocations
inside libclang are not measured, so no bindgen allocation comparison is claimed.

Median requests and requested bytes per complete Toucan generation:

| Header | Requests before | Requests after | Bytes before | Bytes after |
| --- | ---: | ---: | ---: | ---: |
| zlib | 132,232 | 134,294 | 28,650,819 | 29,198,019 |
| SQLite | 116,148 | 116,148 | 29,348,422 | 29,348,508 |
| zstd | 30,149 | 30,149 | 7,309,841 | 7,309,937 |
| libgit2 | 1,381,572 | 1,384,570 | 201,855,181 | 202,521,342 |

Libgit2's parse phase grows from 486,586 to 490,050 requests and from 141,176,605
to 141,879,277 requested bytes. Its binding phase falls from 894,537 to 894,071
requests and from 60,657,471 to 60,620,959 requested bytes. The phase counts alone
do not establish where CPU time is spent. All per-phase measurements and their
observed ranges are retained in `allocations.json.gz`; the capture contains three
randomized process pairs per project and five measured generations per process.

Compiler feature and version declarations changed between the commits, so the
preprocessed inputs need not be identical. Zlib's preprocessed text grows from
31,944 to 33,325 bytes; libgit2's grows from 201,425 to 202,785 bytes. SQLite and
zstd are unchanged. Libgit2's analyzed declaration count changes from 1,465 to
1,461 while its exported bindings remain byte-identical. The header inventory
preserves the preprocessed text and records declarations, records, and enums.

## Identity, inputs, and output checks

Both source snapshots came from exact Git archives. They use separate target
directories. The baseline executable is the previously frozen `a3256d6` benchmark;
its source manifest matches the archived baseline source. The new executable
comes from `a7838ce9d741c8659e428c8a3b2e7f33897bf6fa`. Both binaries record rustc
1.98.1 (`48a229cea`, 2026-09-01), and both use the same benchmark source and lockfile
with bindgen pinned to 0.72.1. The full manifests and binary linkage records are
included.

Before timing, a distinguishing header checks `__GNUC__` and
`__has_builtin(__builtin_prefetch)`: the baseline exposes GNU major 4 and the
unavailable-feature branch; the candidate exposes GNU major 13 and the available
branch. Allocation binaries pass the same control. This supplements executable
hashes and prevents a stale Cargo artifact from masquerading as the candidate.

The target is `x86_64-unknown-linux-gnu`, with the recorded project include paths
and `/` sysroot. Toucan uses its default GNU11 profile; bindgen uses the existing
harness's Clang C11 options. Inputs are untouched public project headers and the
installed system headers. The final inventory records 149 unique input-file
hashes, including libclang's `inttypes.h`, `limits.h`, and `stdint.h`. It combines
Toucan's recorded dependencies with an untimed bindgen include callback. The
callback produces the same Rust bytes as the original harness. The loaded
libclang path was observed in an untimed child process; it is Ubuntu Clang
18.1.3, `/usr/lib/x86_64-linux-gnu/libclang-18.so.18`. All input, executable, and
library hashes were rechecked after the primary capture and at artifact freeze.

Every run checks its generated output against its engine's independently
validated reference. Before/after Toucan outputs are byte-identical. This does
not claim byte or complete API equality between Toucan and bindgen: the known
C-validated macro types, sentinels, SQLite `xDlSym`, and helper/private-field
representation differences remain described in [the benchmark report](../../../docs/benchmarks.md).
This measurement reuses those validated outputs; it does not rerun their FFI and
layout tests.

## Artifacts and reproduction

- `summary.json`: machine-readable results and qualifications.
- `timings.json.gz` and `initial-timings.json.gz`: every raw timing observation,
  command, input hash, executable hash, environment, and CPU/compiler record.
- `allocations.json.gz`: all instrumented generations and phase counts.
- `preflight.json.gz`: identity controls and reference-output checks.
- `header-inputs.json.gz` and `loaded-library.json.gz`: actual input inventory
  and the loaded libclang control.
- `baseline-source.json.gz`, `candidate-source.json.gz`, and
  `binary-linkage.json.gz`: source and executable provenance.
- `freeze-verification.json.gz`: final source, binary, header, library, runner,
  and unchanged omitted-conditional patch checks.
- `capture-artifacts.tar.gz`: exact original capture runners, benchmark and
  counter driver source/manifests/locks, build logs, requests, reference reports,
  all generated Rust outputs, and preprocessed text.
- `capture-artifacts.json`: each archive member's size and SHA-256.

The archived capture runners preserve the original cache paths and commands.
Recreate the source snapshots from the recorded commits and build each benchmark
with its own `CARGO_TARGET_DIR`, `CARGO_INCREMENTAL=0`, and
`cargo build --locked --release --manifest-path benchmarks/inprocess/Cargo.toml`.
The original request files identify each include path, target, sysroot, and
allowlist. Recreate those inputs with the recorded hashes before running the
captures; use fresh capture directories because the runners refuse to overwrite
a previous capture. The archived driver manifests contain absolute dependency
paths to the two snapshots, which must be adjusted if replaying elsewhere.

Run the identity and project-output checks first, then capture the header
inventory, interleave the timing processes, and run the separate allocation
counter. The primary timing runner checks all 149 header hashes and libclang's
hash before and after the run. Preserve any new run separately from this evidence.
