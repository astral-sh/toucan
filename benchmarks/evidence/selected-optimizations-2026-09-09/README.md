# Selected frontend optimizations

This capture compares baseline `52c2ae634a0aeca44b2cdacbd717529b4f5f5df4` with `91d2e313c3084bdd583ebc40d8986ee503bbc11f`, containing the six changes from PRs #359, #361, #362, #363, #364, and #373. SmallVec, ThinVec, and the replacement cache are excluded. Both variants use the system allocator. The earlier nine-change and allocator experiments remain separate captures.

## Results

Binding-generation time falls by 15.2–24.5% across the four header workloads. Retained-analysis construction takes 15.4–19.1% less time. All 35 paired timing comparisons improve. Allocation requests fall by 17.5–51.3%, while requested bytes fall by 5.6–9.3%. Whole-process peak RSS has no consistent improvement.

| Workload | Baseline ms | Selected ms | Paired time change | Allocation requests | Requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| zlib bindings | 22.16 | 17.68 | -16.7% | -45.9% | -9.3% |
| sqlite bindings | 27.70 | 21.42 | -24.5% | -17.6% | -5.6% |
| zstd bindings | 9.22 | 7.59 | -15.2% | -29.2% | -7.2% |
| libgit2 bindings | 196.23 | 158.89 | -16.3% | -40.8% | -7.3% |
| libgit2 retained analysis | 170.07 | 145.93 | -15.9% | -46.8% | -8.7% |
| zlib retained analysis | 21.94 | 17.72 | -19.1% | -48.5% | -9.0% |
| zlib-adler32 retained analysis | 37.92 | 31.99 | -15.5% | -51.3% | -9.2% |

The percentage changes are medians of paired ratios, so they can differ from dividing the two displayed median times. [Full results](summary.md) include every observed range. These are descriptive measurements on a shared Linux host; they do not establish statistical significance or cross-platform speedups.

The baseline was frozen before subsequent target-guard, empty-macro whitespace, and CodSpeed changes landed on main. This comparison measures the six optimizations at the recorded revisions.

## Method and validation

The helper functions and allocation counter are byte-identical to the original frozen driver. All 40 shared registry packages remain pinned, including SmallVec 1.15.2. The selected variant adds only the four pinned CharStr packages. Both variants use the same compiler, fat LTO, one codegen unit, and disabled Ohm experimental defaults.

Seven workloads cover unchanged zlib, SQLite, zstd, and libgit2 headers, retained analysis of zlib and libgit2 headers, and retained analysis of zlib’s untouched `adler32.c`. All 136 physical inputs and request files are checked before and after each capture.

Each timing process discards one warmup and measures five calls. Five rounds shuffle workload and variant order, with processes pinned to CPU 3. Separate allocation-counting binaries run three rounds of three measured calls after warmup. All 350 timed calls and 126 counted calls reproduce the fresh baseline outputs. Builder output and normalized reports also match the earlier C-validated references.

Allocation counters report allocation/reallocation requests and requested sizes, including the full new size of reallocations. They do not measure live or peak heap. Process RSS includes verification and graph serialization. Checked timings stop when `parse_file` returns its retained `Compilation`, excluding serialization and destruction. Checked snapshots compare the translation unit, checked graph, preprocessed source, and dependency paths; macro maps, source mappings, and optional catalogs are outside this comparison.

The selected revision passes 1,105 workspace tests with all features, with zero failures and 266 ignored tests. Workspace Clippy with all features and all targets passes with warnings denied, as does formatting. Validation logs are retained.

## Reproduction and archive verification

The archive contains the capture scripts, frozen helper source and locks, build commands, source revisions, raw outputs, normalized reports, counters, host context, and validation receipts. Binaries and original input files are identified by SHA-256 and size. Recreating the benchmark requires restoring the recorded revisions and adapting absolute source and sysroot paths.

Keep the artifact sidecars together and verify it offline with:

```console
python3 package_capture.py verify capture.json.gz
```

The verifier checks hashes, complete workload coverage, dependency pins, output comparisons, and recomputes the summary. It does not rebuild the frontend or rerun physical inputs absent from the archive.

Two failed harness attempts are preserved under `attempts/`: the first compared the newer 136-file inventory with an older 132-file preflight inventory, and the second failed to import a sibling helper. Both were corrected before the successful capture. Failed attempts and build preflight timings are excluded from the reported measurements.
