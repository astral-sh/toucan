# Builder baseline

On September 9, 2026, the frozen `acfb815` frontend completed a paired comparison
with bindgen 0.72.1 and libclang 18.1.3 on the four pinned upstream header sets.
Both Builder APIs used the same 73 physical public-header roots, target, includes,
comments, derives, macro and enum policies, Rust 1.64 output, and no formatter.
The [preflight](../builder-preflight-acfb815/README.md) records the structural,
native C, Rust compilation, and actual FFI checks, including the remaining API
differences. This is a baseline before the array-parameter typedef fix.

| Project | Toucan Builder | bindgen | Median paired speedup | Paired speedup range |
| --- | ---: | ---: | ---: | ---: |
| zlib 1.3.1 | 21.82 ms | 116.11 ms | 5.30× | 5.19–5.51× |
| SQLite 3.45.1 | 25.67 ms | 154.34 ms | 6.27× | 5.74–6.44× |
| zstd 1.5.7 | 9.09 ms | 103.56 ms | 11.53× | 11.09–12.33× |
| libgit2 1.9.1 | 192.71 ms | 261.73 ms | 1.37× | 1.32–1.41× |

Each time is the median of seven process medians. Each process makes one discarded
first call and ten measured calls. Speedup is the median of the seven ratios
within matched process pairs; it is not the ratio of the two displayed medians.
The range contains all seven pair ratios. Engine order is randomized within each
pair with seed `20260909`; project order is zlib, SQLite, zstd, libgit2. The capture
contains all 560 measured calls, 56 first calls, and generated outputs.

## First call in each process

| Project | Toucan Builder | bindgen |
| --- | ---: | ---: |
| zlib | 26.42 ms | 150.69 ms |
| SQLite | 34.67 ms | 191.94 ms |
| zstd | 12.77 ms | 136.71 ms |
| libgit2 | 217.02 ms | 324.37 ms |

These are the medians of seven first calls. They include initial libclang loading
and initialization, but exclude process startup. Filesystem caches were warm.
They do not measure cold machine starts or complete build-script execution.

## Scope and reproducibility

The executable used Rust 1.98.1, the checked-in lockfile, a fresh release target
directory, and the system allocator. Calls were pinned to CPU 3. Our native tests
and sanitizer jobs had finished, and implementation probes and builds were paused.
Lightweight metadata, file, and GitHub work continued. Other tenants, CPU frequency,
memory bandwidth, and filesystem activity remain uncontrolled on this shared
Linux host. CPU frequency and governor files were unavailable.

The runner requires each engine's output hash and configuration to match its
untimed capture after every process. It checks all header dependency hashes before
and after timing, and checks the executable again at the end. The packaged evidence
also verifies the libclang checksum and all 2,301 original source files after the
run. The previously recorded generated Python bytecode remains a separate extra.

Exact API comparison passes for zstd. The baseline omits `va_list` and
`__builtin_va_list` aliases in zlib and SQLite, retains a C-validated correction to
SQLite's returned callback, and emits twelve additional libgit2 aliases. Three
sentinel constants use bindgen-compatible signed Rust values and require an
explicit unsigned conversion to equal the native C value. The preflight retains
the native probes and every difference. These results do not establish complete
API equivalence or application speedups.

The [summary](summary.json) gives per-process medians, paired ratios, first calls,
environment and source identities, and artifact checksums. The
[capture](capture.json.gz) preserves all samples, exact commands and requests,
outputs, input hashes, build provenance, and loaded libclang identity. The earlier
core-route measurements use different selection and output policies and remain a
separate workload.
