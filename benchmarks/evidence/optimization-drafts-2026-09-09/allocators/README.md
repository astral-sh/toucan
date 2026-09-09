# Toucan allocator benchmark evidence

All six allocator/source combinations passed preflight and five timing rounds: 1,050 measured calls across seven workloads. The complete [results](summary.md) include paired ranges and comparisons with each allocator held fixed.

The experiment compares System, jemalloc, and mimalloc with both the baseline
frontend and the final optimization stack. Seven workloads cover Builder calls
for untouched zlib, SQLite, zstd, and libgit2 headers; retained checked analysis
for zlib and libgit2 headers; and retained analysis of zlib's untouched `adler32.c`.
The translation unit exercises function bodies and nonempty conversion lists.

`summary.md` and `summary.json` report median process timings, allocator changes
against System at the same frontend revision, and frontend changes with the
allocator held fixed. Ratios pair observations from the same round and workload.
Ranges are descriptive minima and maxima, not confidence intervals. These are
local measurements, with no claim about CI performance or statistical significance.

RSS covers the entire benchmark process, including output verification and
checked-graph JSON serialization. It does not measure frontend-only peak memory
or the retained heap. This study does not use the separate System allocation
counter and does not report allocation counts for custom allocators.

Checked-mode timings cover `parse_file` through the return of its retained
`Compilation`. They exclude both destruction of that compilation and JSON
serialization. Checked snapshots compare the translation unit, checked graph,
preprocessed source, and dependency paths. They do not compare macro maps,
source mappings, or optional catalogs. These output checks therefore establish
equivalence only for the captured data.

## Results on the optimized frontend

Mimalloc reduces header binding-generation time by 10.3–18.5% relative to System; jemalloc reduces it by 6.6–15.4%. Mimalloc raises Builder process RSS from 10–39 MiB to 24–63 MiB. Retained analysis of `adler32.c` has a 2.8% median slowdown with mimalloc, and all five paired rounds are slower. Process RSS rises from 40.79 to 65.22 MiB. Jemalloc makes that analysis 3.2% faster, while process RSS rises to 72.65 MiB.

| Workload | System ms | System RSS MiB | Jemalloc time change | Jemalloc RSS MiB | Mimalloc time change | Mimalloc RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| zlib bindings | 18.00 | 11.00 | -6.6% | 12.25 | -10.4% | 24.48 |
| sqlite bindings | 21.34 | 15.42 | -15.4% | 15.25 | -18.4% | 30.25 |
| zstd bindings | 7.36 | 10.12 | -11.6% | 11.75 | -18.5% | 24.22 |
| libgit2 bindings | 157.97 | 38.89 | -8.6% | 44.96 | -10.3% | 62.72 |
| libgit2 retained analysis | 135.40 | 153.27 | -4.0% | 179.50 | -2.2% | 158.65 |
| zlib retained analysis | 17.26 | 26.14 | -2.4% | 59.36 | -6.4% | 49.20 |
| zlib-adler32 retained analysis | 31.51 | 40.79 | -3.2% | 72.65 | +2.8% | 65.22 |

These tradeoffs support keeping allocator selection with the embedding application and measuring its actual workload. The draft adds optional allocator features to the standalone benchmark, with System as its default. It changes no frontend library allocator policy.

## Offline verification

Keep the artifact's sidecars together, then run:

```console
python3 package_capture.py verify capture.json.gz
```

The verifier checks the manifest, stored content hashes and counts, complete
workload coverage, output and normalized-report equality against included primary
baseline goldens, and raw RSS records. It uses the hash-checked `summarize.py`
sidecar to recompute all summary arithmetic and to verify source revisions,
compiler settings, dependency locks, and unchanged benchmark function bodies.
Neither the original checkout nor the original input files are needed.

The content-addressed gzip JSON archive contains raw captures, every generated
output comparison, normalized reports, request JSON, frozen helper source and
dependency locks, build metadata and logs, and the primary baseline references.
`primary/host.json` records the host CPU and libc information.
Repeated identical files share one stored content block. `manifest.json` records
compressed, logical, and unique byte counts and SHA-256 hashes.

Original executable binaries and physical header/source files are recorded by
SHA-256 and size. They are not embedded. Checked output snapshots contain the
preprocessed source recorded by the benchmark. Offline verification confirms
archive integrity and the recorded comparisons; it does not rerun the frontend,
rebuild executables, or revalidate files absent from the archive.

## Package completed captures

From the allocator work directory, after generating both summary files:

```console
python3 package_capture.py package
```

The command verifies current evidence and external input/binary hashes, then
creates a new `packaged` directory. It refuses to overwrite an existing artifact.
