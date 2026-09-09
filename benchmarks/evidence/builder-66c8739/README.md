# Builder generation at `66c8739`

On September 9, 2026, the frozen
[`66c87396`](https://github.com/astral-sh/toucan/commit/66c87396bbe03b22085339a87b8c350c90be280b)
Builder completed a fresh comparison with bindgen 0.72.1 and libclang 18.1.3.
Both generators used the same 73 physical public-header roots, include paths,
comments, derives, macro and enum policies, Rust 1.64 output target, and disabled
formatting. The untimed preflight generated and validated fresh reference outputs
before timing.

| Project | Toucan Builder | bindgen | Median paired ratio | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| zlib 1.3.1 | 22.224 ms | 117.933 ms | 5.30× | 5.22–5.56× |
| SQLite 3.45.1 | 25.364 ms | 154.921 ms | 6.10× | 5.79–6.30× |
| zstd 1.5.7 | 9.121 ms | 101.911 ms | 11.23× | 10.77–11.48× |
| libgit2 1.9.1 | 192.849 ms | 263.189 ms | 1.37× | 1.34–1.39× |

Times are medians of seven process medians. Each process records one first call,
then ten measured calls. The ratio is the median of seven within-pair
bindgen/Toucan ratios; it need not equal the ratio of the displayed medians.
Engine order is shuffled within each pair using seed `20260909`. Project order
is zlib, SQLite, zstd, libgit2. All 560 measured calls, 56 first calls, and 56
generated timing outputs are retained.

| First call in each process | Toucan Builder | bindgen |
| --- | ---: | ---: |
| zlib | 26.265 ms | 153.608 ms |
| SQLite | 35.033 ms | 194.471 ms |
| zstd | 12.883 ms | 135.056 ms |
| libgit2 | 216.161 ms | 322.957 ms |

These first-call medians include library loading and initialization; process
startup is excluded. Filesystem caches were warm.

## Validation

The fresh preflight completed 110 commands successfully. It compiled the exact
generated modules with Rust 1.98.1 and actual Rust 1.64.0, then ran eight Rust FFI
executables against the pinned C archives. Independent GCC 13.3.0 and Clang 18.1.3
probes covered 17 C records, 88 field offsets, and 1,293 constants for each engine.
Comparisons between generated Rust modules covered 1,288 shared functions,
three globals, 1,293 constants, 115 record layouts, and 667 field offsets.
The comparison found no unsupported items or omitted selected declarations.

Structural API equality holds for zlib and zstd. Remaining differences are:

- SQLite's `sqlite3_vfs.xDlSym` returns a callback with a different Rust signature;
  Toucan retains the previously C-validated correction.
- Toucan emits ten additional integer typedef aliases for libgit2.
- Both generators emit signed Rust values for `ZSTD_CONTENTSIZE_UNKNOWN`,
  `ZSTD_CONTENTSIZE_ERROR`, and `GIT_REBASE_NO_OPERATION`, whose C expressions
  are unsigned 64-bit values. The fresh C probes retain those value differences.

All eight fresh preflight output files are byte-identical to the corresponding
[earlier capture](../builder-preflight-callbacks/README.md), as are the structural
comparison results and native C/Rust observations. This establishes continuity of
the observed output differences; the new timing uses the newly built executable.

## Scope

The binary used Rust 1.98.1, the checked-in lockfiles, a fresh normal-toolchain
production release build, and the system allocator. Experimental Ohm defaults,
trust settings, inherited compiler flags, and allocator overrides were cleared.
Calls were pinned to CPU 3 on a shared AMD EPYC-Milan Linux host. Other local
compiler work completed before timing. CPU frequency, other tenants, memory
bandwidth, and filesystem contention remain uncontrolled.

These timings cover Builder configuration, preprocessing, parsing, semantic
analysis, and Rust string generation. They exclude process startup, formatting,
file writing, native-library compilation, and complete consumer builds. They do
not measure uv runtime performance or establish an improvement over older Toucan
revisions. Claims apply to this source commit and these four workloads; arbitrary
headers, other targets, traits, and bitfield accessors need their own evidence.

## Evidence and reproduction

[summary.json](summary.json) contains the reviewed measurements, ranges, first
calls, toolchain versions, limitations, and integrity checks.
[capture.json.gz](capture.json.gz) retains every text artifact from the fresh
preflight and timing, along with exact commands, requests, generated outputs,
native probe sources/results, reviews, reproduction drivers, and the SHA256
manifest of all 2,529 files in the frozen source tree. Identical text is stored
once by content hash; `files` maps original paths to those entries. Source code
for the benchmark, comparison tools, and corpus fixtures is included. The rest of
the source is identified by its Git commit/tree and full file manifest.

Compiler outputs, toolchain binaries, native archives, and the complete source
tree are not duplicated. Their identities and available hashes remain recorded;
the archive does not independently recreate the build environment. Absolute
paths reflect the measured machine. Reproduction elsewhere requires preparing
the same pinned dependencies and explicitly recording any relocated input paths.
The captured `drivers/refresh.py` separates source freezing, production preflight,
and reviewed timing. Read `reproduction/README.md` inside the capture before use.

Verify package hashes, every retained artifact, output/configuration consistency,
the seeded process order, and all paired statistics without compiling or timing:

```console
python3 -B benchmarks/evidence/builder-66c8739/verify.py
```

The recorded audit checked every timed output and configuration against its new
preflight, then rechecked source, headers, native archives, executable, libclang,
and linked dynamic-library hashes. Packaging performs no new benchmark or probe
executions and preserves the original artifacts.
