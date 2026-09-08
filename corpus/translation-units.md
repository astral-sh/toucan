# Complete C translation units

This audit checks untouched source files from the four pinned projects. It covers
zlib `adler32.c` and `deflate.c`, libgit2 `src/util/alloc.c` and
`src/libgit2/repository.c`, zstd `lib/common/zstd_common.c` and
`lib/compress/zstd_compress.c`, and SQLite's generated `sqlite3.c`.
The [manifest](translation-units.json) pins archives and source files. Generated
SQLite output gets a recorded hash because generation tools can vary by platform.

Run on native Linux with Python 3.12, CMake, make, a C compiler, and the other
[corpus build prerequisites](README.md):

```sh
python3 scripts/prepare_corpus.py --cache corpus/cache
cargo build --locked --release -p toucan --example audit_translation_unit
python3 -m unittest discover -s scripts/tests -v
python3 scripts/audit_translation_units.py \
  --cache corpus/cache \
  --probe target/release/examples/audit_translation_unit \
  --revision "$(git rev-parse HEAD)" \
  --output corpus/results/translation-units
```

The output directory must be new. Use `prepare_corpus.py --offline` with cached
archives. Older prepared caches need preparation again to export compile commands
and the zstd make dry run. The audit never rewrites source files or headers.

## Commands and inputs

CMake compilation databases select the exact static-library target and source.
The zstd command comes from a forced verbose make dry run using the compiler and
variables from its completed build. SQLite uses the explicit command recorded by
preparation. Compiler checks keep the complete build flags, dropping only object
outputs and dependency-generation options. The report records both commands.

The Toucan route applies build `-I`, `-D`, and `-U` options in order and discovers
system include paths from that compiler. It uses Toucan's shipped target feature
profile. Compiler optimization, language-mode, warning and other flags outside
that profile are listed explicitly. Unsupported include-option forms stop the
audit. `LC_ALL=C` and `SOURCE_DATE_EPOCH=0` make diagnostics and date macros stable.

Source-tree dependencies are checked against the pinned archive members. Generated
headers, system headers, embedded Toucan headers, preprocessing output, compile
command provenance, build logs, and the probe executable all have recorded hashes.
Dependencies and the probe are checked again after analysis to catch changes.

## Routes and pass criteria

Each input runs twice through `toucan::parse_file`: with retention disabled and
enabled. A separate route passes the compiler's unchanged `-E` output through the
same public integration API, preserving GNU line markers. Every route compares
acceptance, rejection diagnostics, and the hash of the complete declaration IR.
The probe streams IR to a sidecar; reaching its 256 MiB cap is an explicit error.
It never compares truncated prefixes or serializes the full retained graph.

The default gate requires all seven Toucan-route pairs to succeed and both routes
to have retention parity. Compiler-preprocessed rejections remain explicit in
`routes.compiler_preprocessed`; they do not count as accepted translation units.
`--require both` requires both routes to succeed. `--require parity` is an
exploratory gate that allows matching rejections, which remain visible in the
report. `compatibility_status` and the per-route completion counts distinguish
complete compatibility from a passed, narrower gate.

Resource limits are pinned in the manifest and copied into every request. The
2,000,000 preprocessing-token limit matches the CLI default. SQLite's retained
analysis requires more than the library's default 1,000,000 nodes and 4,000,000
edges; this audit permits 2,000,000 nodes and 8,000,000 edges, keeping the default
128 MiB charged-payload limit. These are resource settings, not feature overrides.

Elapsed time and peak resident memory describe individual audit processes. A
failed or timed-out analysis is incomplete work and is never a throughput sample.
No compiler speed ratio is computed. This seven-file sample does not establish
whole-project acceptance or optimizer/code-generator equivalence.

## Recorded result

The [Linux x86-64 evidence](evidence/translation-units-2026-09-08.json) checks frontend
revision `f78baa8dc399daceabeeead4878482fc7f53a33e`. All seven Toucan-route inputs
complete in both modes with identical declaration hashes. All fourteen route pairs
have retention parity. The compiler-preprocessed route accepts the two zlib inputs;
libgit2 and SQLite stop at `__atomic_store_n`, and zstd stops at
`__builtin_ia32_emms`. These five rejections remain visible compatibility gaps.

The evidence uses path parameters for cache, checkout, output and probe locations,
and shares one indexed table of dependency hashes. Full logs, preprocessing files,
requests and declaration sidecars are emitted by the harness and uploaded by CI.

### Observed elapsed time

Toucan's normal route took longer than GCC syntax checking on six of these seven
invocations; `adler32.c` was approximately equal. The earlier speed claims concern
subprocess binding generation, not complete C-body analysis.

| Translation unit | GCC syntax check (s) | Toucan normal (s) | Toucan retained (s) |
| --- | ---: | ---: | ---: |
| zlib-adler32 | 0.065 | 0.065 | 0.115 |
| zlib-deflate | 0.065 | 0.115 | 0.166 |
| libgit2-alloc | 0.115 | 0.267 | 0.266 |
| libgit2-repository | 0.216 | 0.567 | 0.667 |
| zstd-zstd_common | 0.065 | 0.115 | 0.165 |
| zstd-zstd_compress | 0.166 | 0.366 | 0.516 |
| sqlite-sqlite3 | 0.724 | 2.377 | 4.142 |

These are single audit observations, not a controlled throughput benchmark. Times
include process startup and timeout supervision; Toucan also streams declaration
IR for fingerprinting. GCC uses the project's complete build flags, while Toucan
uses its separately recorded header feature profile. Short timings are affected
by supervision granularity and ordinary run-to-run variation. The failing
compiler-preprocessed routes are excluded from this table.
