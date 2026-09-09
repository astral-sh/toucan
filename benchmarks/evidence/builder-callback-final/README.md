# Builder generation with callback typedef support

On September 9, 2026, the frozen `7db2b850` Builder completed a paired comparison
with bindgen 0.72.1 and libclang 18.1.3. Both generators used the same 73 physical
public-header roots, includes, comments, derives, macro and enum policies, Rust
1.64 output target, and no formatter. The [preflight](../builder-preflight-callbacks/README.md)
records structural, native C, Rust compilation and actual FFI checks.

| Project | Toucan Builder | bindgen | Median paired speedup | Paired speedup range |
| --- | ---: | ---: | ---: | ---: |
| zlib 1.3.1 | 22.05 ms | 118.03 ms | 5.34× | 5.28–5.49× |
| SQLite 3.45.1 | 26.07 ms | 156.25 ms | 5.98× | 5.70–6.10× |
| zstd 1.5.7 | 9.02 ms | 104.85 ms | 11.62× | 11.04–12.04× |
| libgit2 1.9.1 | 194.20 ms | 263.32 ms | 1.36× | 1.34–1.37× |

Times are medians of seven process medians. Each process records one first call,
then ten measured calls. Speedup is the median of the seven ratios within matched
process pairs; the range includes every pair. Engine order is shuffled within
each pair with seed `20260909`; project order is zlib, SQLite, zstd, libgit2.
The capture retains all 560 measured calls, 56 first calls and 56 generated files.

## First call in each process

| Project | Toucan Builder | bindgen |
| --- | ---: | ---: |
| zlib 1.3.1 | 26.68 ms | 154.70 ms |
| SQLite 3.45.1 | 34.66 ms | 189.91 ms |
| zstd 1.5.7 | 12.85 ms | 137.12 ms |
| libgit2 1.9.1 | 218.02 ms | 323.25 ms |

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
the executable, libclang and all 2,389 files in the complete source snapshot stayed unchanged.
The [summary](summary.json) includes all process medians, pair ratios, first-call
samples and ranges. The [capture](capture.json.gz) preserves exact drivers,
commands, requests, outputs, raw samples, audit records and source manifests.

The measured commit is the exact validated and published
[`7db2b850`](https://github.com/astral-sh/toucan/commit/7db2b85027108be1515ad76263720e665cba2ab9),
including nullable callback typedefs, alias-cycle validation, and the generation
work budget. Earlier [selection timings](../builder-selection/README.md) remain
separate evidence. These shared-host runs do not establish a statistically
significant revision change or performance for later implementation layers.

Structural API equality holds for zlib and zstd. SQLite retains its C-validated
returned-callback correction; libgit2 retains ten extra integer typedef aliases.
Three bindgen-compatible signed sentinel constants require unsigned conversion
to equal their native C values. The preflight preserves each difference. These
measurements cover binding generation, not complete application performance.
