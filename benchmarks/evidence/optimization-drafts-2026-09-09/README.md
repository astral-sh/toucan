# Frontend optimization drafts

Nine cumulative draft changes compare parser metadata hashing, short semantic lists, token spellings, shared macro suppression names, source scanning, macro copies, replacement caching, persistent syntax lists, and SIMD byte searches. The baseline is `0a47f9781d92746bb97184a447aea5965b9dafb9`; source revisions and binary hashes are recorded separately for every step.

## Results

The combined stack reduces median paired binding-generation time by 15.2–17.8% across the four header workloads. Retained-analysis construction takes 17.4–20.8% less time across the three checked workloads. Allocation requests fall by 20.9–55.6%, while requested bytes fall by 8.0–12.3%. Whole-process peak RSS changes by less than 1% on every workload.

| Workload | Baseline ms | Optimized ms | Paired time change | Allocation requests | Requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| zlib bindings | 21.48 | 17.82 | -17.8% | -49.2% | -11.9% |
| sqlite bindings | 24.80 | 20.38 | -15.2% | -20.9% | -8.0% |
| zstd bindings | 8.80 | 7.29 | -17.4% | -34.4% | -10.5% |
| libgit2 bindings | 189.84 | 157.95 | -17.6% | -43.9% | -9.5% |
| libgit2 retained analysis | 167.56 | 133.63 | -20.8% | -49.5% | -11.1% |
| zlib retained analysis | 21.37 | 17.47 | -18.9% | -51.7% | -11.6% |
| zlib-adler32 retained analysis | 38.55 | 31.01 | -17.4% | -55.6% | -12.3% |

The byte scanner reduces per-workload median paired time by 5.3–10.8%, and compact token strings reduce it by 1.9–6.8%. SIMD reduces header binding time by a further 3.2–5.4%; its `adler32.c` result is essentially unchanged (+0.07%). SmallVec reduces allocation requests but increases all four header medians by 0.4–2.5%. ThinVec reduces requested bytes by 0.7–2.1%, with mixed timings and no consistent RSS reduction. Its public AST field changes therefore need a separate API tradeoff review.

The complete [per-draft tables](summary.md) retain negative results and observed round ranges. These are descriptive measurements from one shared Linux host; they do not establish statistical significance or cross-platform speedups. All nine experiments remain drafts.

## Method

The timing driver configures and generates bindings from the same untouched zlib, SQLite, zstd, and libgit2 header requests as the earlier Builder capture. Three additional Clang C11 workloads retain the complete checked graph for zlib headers, libgit2 headers, and untouched zlib `adler32.c`. The function-body workload includes five bodies, 585 expressions, and nonempty conversion lists; it uses the minimal Clang C11 driver configuration, not the upstream CMake build flags. Checked-graph serialization, output comparison, and destruction of the retained Compilation happen after each timed call. The checked results measure construction of the retained analysis. These workloads exercise retained analysis independently of binding generation.

Each binary uses the system allocator, fat LTO, one codegen unit, and the same Ohm 1.98.1 compiler with experimental Cargo defaults disabled. These are local draft comparisons, not a new comparison with bindgen or application build times. The host is a shared AMD EPYC Linux machine. Timed processes are pinned to CPU 3; other tenants, frequency, and memory bandwidth are uncontrolled. Filesystem caches are warm.

Every process discards one first call, then measures five calls. Five rounds shuffle both workload order and variant order within each workload. Reported times are medians of process medians. Incremental ratios pair each complete draft PR with its parent in the same round; combined ratios compare the final draft with the baseline. The SmallVec PR also upgrades an existing transitive SmallVec dependency from 1.15.2 to 1.16.0, including its use by rustc_apfloat. Its measurements include that dependency upgrade and do not isolate the container substitution. Raw first calls, samples, process medians, and paired ranges are retained.

Allocation observations use separately built binaries with a counter that delegates to System. They count allocation/reallocation requests and requested bytes, including the full new size of each reallocation. Their elapsed times are excluded from timing comparisons. These counters do not measure live or peak heap. Linux process RSS includes the driver, retained reference output, and verification; checked-mode RSS also includes complete graph serialization and must not be called frontend-only peak memory.

## Equivalence and validation

Each timed call reproduces its process reference output. Each draft's complete Builder output matches both the current baseline and the corresponding earlier C-validated output. Reports match after removing only timings. Checked snapshots retain the translation unit, preprocessed source, graph, and actual dependency paths and match the baseline exactly. These snapshots do not compare macro environments, source mappings, or optional inspection catalogs. Physical input hashes are checked before and after each capture.

The integrated implementation through ThinVec passes 1,098 workspace tests with all features, zero failures, and 263 ignored tests. The subsequent SIMD change passes 111 preprocessor tests, with zero failures and 17 ignored tests, plus scoped Clippy. Workspace Clippy with all features/all targets and warnings denied passes, as does formatting. The scalar scanner has 40,960 comparisons with the original implementation; SIMD coverage brings that total to 75,520, including vector-width and tail boundaries. Separate preprocessor tests include native GCC/Clang feature-query and pragma matrices. This does not claim new full negative-C conformance, application integration, or cross-platform performance coverage.

Five public parser AST extension fields change from Vec to ThinVec. The facade and checked-analysis slice accessors keep their existing interfaces. The drafts are experimental and remain unmerged. The allocator benchmark is a tenth draft; its separate capture compares System, jemalloc, and mimalloc on both the baseline and complete optimized frontend. Allocator features belong to the standalone benchmark executable. Embedding applications retain control of their global allocator.

## Reproduction

The capture preserves the exact helper source, resolved lockfiles, compiler details, build commands, input requests, source revisions, and binary hashes. Recreate each source revision, adapt the recorded absolute paths to the local checkout/sysroot/corpus, and build separate timing and allocation binaries with the recorded settings. Run output preflight before measuring. Do not treat relocated or changed header inputs as the same capture.

The disk filled during one intermediate build and interrupted its checkout. That worktree and failed attempt were preserved, and remaining variants were rebuilt in a fresh worktree. Failed builds and preflight-only timings are excluded from these measurements; source paths, completed build receipts, and the incident are recorded in the archive.
