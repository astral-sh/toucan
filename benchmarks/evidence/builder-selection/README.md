# Builder generation after the selection fixes

On September 9, 2026, the frozen `43867b9` Builder completed a paired comparison
with bindgen 0.72.1 and libclang 18.1.3. Both generators used the same 73 physical
public-header roots, includes, comments, derives, macro and enum policies, Rust
1.64 output target, and no formatter. The [preflight](../builder-preflight-selection/README.md)
records structural, native C, Rust compilation and actual FFI checks.

| Project | Toucan Builder | bindgen | Median paired speedup | Paired speedup range |
| --- | ---: | ---: | ---: | ---: |
| zlib 1.3.1 | 22.04 ms | 116.35 ms | 5.29× | 5.20–5.35× |
| SQLite 3.45.1 | 25.91 ms | 153.45 ms | 5.98× | 5.70–6.32× |
| zstd 1.5.7 | 8.85 ms | 106.08 ms | 11.87× | 11.59–12.38× |
| libgit2 1.9.1 | 192.22 ms | 262.30 ms | 1.36× | 1.32–1.43× |

Times are medians of seven process medians. Each process records one first call,
then ten measured calls. Speedup is the median of the seven ratios within matched
process pairs; the range includes every pair. Engine order is shuffled within
each pair with seed `20260909`; project order is zlib, SQLite, zstd, libgit2.
The capture retains all 560 measured calls, 56 first calls and 56 generated files.

## First call in each process

| Project | Toucan Builder | bindgen |
| --- | ---: | ---: |
| zlib 1.3.1 | 26.77 ms | 154.99 ms |
| SQLite 3.45.1 | 34.47 ms | 189.13 ms |
| zstd 1.5.7 | 12.88 ms | 136.62 ms |
| libgit2 1.9.1 | 216.16 ms | 327.76 ms |

These are medians of seven first calls, including initial library loading and
initialization. Process startup is excluded. Filesystem caches were warm.

## Scope and reproducibility

The binary used Rust 1.98.1, a fresh standalone release build, the locked
dependencies and system allocator. Calls were pinned to CPU 3 on an AMD EPYC-Milan
Linux host. The native suite, sanitizer validation and consumer builds completed
before timing; all agents paused compiler and probe work during the run.
CPU frequency, other tenants, memory bandwidth and filesystem contention remain
uncontrolled. All samples, including outliers, are retained.

Every process's output hash and configuration match its reviewed preflight.
The runner checks header hashes before and after measurement. The audit confirms
the executable, libclang and both complete source snapshots stayed unchanged.
The [summary](summary.json) includes all process medians, pair ratios, first-call
samples and ranges. The [capture](capture.json.gz) preserves exact drivers,
commands, requests, outputs, raw samples, audit records and source manifests.

The measured library code matches validated `00ec563` and published
[`b371cd0`](https://github.com/astral-sh/toucan/commit/b371cd0cdda8e5cc4190c42f47d510f3b9115aac).
This capture predates the nullable callback typedef changes. It does not establish
performance for later layers or statistical significance relative to the earlier
[`acfb815` capture](../builder-acfb815/README.md) on this shared host.

Structural API equality holds for zlib and zstd. SQLite retains its C-validated
returned-callback correction; libgit2 retains ten extra integer typedef aliases.
Three bindgen-compatible signed sentinel constants require unsigned conversion
to equal their native C values. The preflight preserves each difference. These
measurements cover binding generation, not complete application performance.
