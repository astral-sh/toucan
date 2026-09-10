# In-process binding benchmarks

This harness has two separate comparisons. `policy: "legacy_core"` preserves the
older `toucan` core-library route and its signed macro, unprefixed enum, and
comment-free bindgen reference. `policy: "builder"` compares `toucan-builder`
with bindgen 0.72.1 through their Builder APIs. The new route requires explicit
physical file or category-specific name roots on both sides. It does not reuse
the older name-root scope or its output hashes.

## Matched Builder policy

| Setting | Both implementations |
| --- | --- |
| Macro integers | Unsigned default; `fit_macro_constants(false)` |
| Enum constants | `prepend_enum_name(true)`; ordinary integer enums |
| Comments | Enabled; a separate request may explicitly disable them |
| Derives | Copy and Debug enabled; Default, Eq, PartialEq disabled |
| Target and language | Explicit request target, `-x c -std=c11`, same sysroot and ordered `-I` paths |
| Selection | Identical physical file, type, function, and variable regexes |
| Rust output | Rust 1.64, core paths, `size_t_is_usize(true)` |
| Layout tests | Disabled; Toucan's compile-time layout assertions remain |
| Formatting | `Formatter::None` on both sides |
| Callbacks | None |

The macro, enum, comment, and derive choices match bindgen defaults. Rust version,
core paths, disabled runtime layout tests, formatting, and file selection are
explicit common overrides. This measures header processing and string generation,
not rustfmt, file writing, native library compilation, or a complete consumer build.
Toucan's Builder separately requests Debug by default; the explicit setting here
keeps the benchmark configuration stable across implementations.

## Freeze and input preparation

Do not time a working tree while it is being edited. Build the final committed
source in its own source and target directories with the checked-in lockfile:

```console
CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/tmp/toucan-final-benchmark-target \
  cargo build --locked --release --manifest-path benchmarks/inprocess/Cargo.toml
```

Use the same rustc and lockfile for any additional Toucan baseline, with a separate
source/target directory. Record the exact commit, source manifest, build command,
rustc version, executable hash, linked libraries, loaded libclang path/hash/version,
and Clang resource directory. The timing executable uses the system allocator;
record allocator linkage and verify no `LD_PRELOAD` or allocator override is active.
Allocation instrumentation belongs in a separate executable and capture. Rust
allocation counts do not include libclang's native C++ allocations.

Start with the existing zlib 1.3.1, SQLite 3.45.1, zstd 1.5.7, and libgit2 1.9.1
input paths, include order, target, and sysroot recorded in the earlier benchmark
requests. Replace the inherited GCC 13 compiler-resource include directory with
the installed Clang 18 resource include directory on both Builder routes; keep
project include order and sysroot unchanged and record this change. Create new
requests instead of modifying historical evidence. For example:

```json
{
  "header": "/absolute/path/zlib-1.3.1/zlib.h",
  "target": "x86_64-unknown-linux-gnu",
  "include_dirs": ["/absolute/path/build/zlib", "/absolute/path/zlib-1.3.1", "/usr/lib/llvm-18/lib/clang/18/include"],
  "sysroot": "/",
  "policy": "builder",
  "allowlist_files": ["^/absolute/path/zlib\\-1\\.3\\.1/zlib\\.h$", "^/absolute/path/build/zlib/zconf\\.h$"],
  "generate_comments": true
}
```

Use every project header reached by the main header as a separate exact file
root, including zconf.h for zlib and the included public headers behind libgit2's
git2.h umbrella. Selecting only that umbrella would yield almost no API. Preserve
the compiler-visible physical paths and escape regex metacharacters. Record
all project and system header hashes, including libclang resource headers. An
untimed include callback may collect dependencies only if its output is byte
identical to the callback-free primary generation: callback presence can change
macro evaluation. Otherwise use the matching Clang dependency command and
Toucan's preprocessor accessed-file inventory separately, preserving their flags
and limitations. Recheck all input and executable/library hashes after capture.

To measure name selection, supply `allowlist_types`, `allowlist_functions`, or
`allowlist_vars` arrays instead of, or alongside, `allowlist_files`. Both Builders
receive the same category-specific patterns. For example, a zstd request can use
`"allowlist_functions": ["ZSTD_.*"]`. Each selection is a separate workload and
requires its own output comparisons and native checks before timing. The older
`allowlist` and `bindgen_allowlist` fields remain exclusive to `legacy_core`.
The [untimed control capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/benchmarks/evidence/name-filter-controls-2026-09-09.json.gz)
records matching type, function, variable, and combined selections, plus rejection
of empty roots and mixed policies. It includes generated Rust and native Rust
layout/value probes, without latency samples or C FFI executions.

## Untimed acceptance gate

The `capture` command makes one generation and emits no timing samples:

```console
/path/to/toucan-inprocess-benchmark toucan-builder zlib.request.json capture zlib-toucan.rs
/path/to/toucan-inprocess-benchmark bindgen zlib.request.json capture zlib-bindgen.rs
```

Require expected project entry points such as deflate/inflate, sqlite3_open,
ZSTD_compress, and git_libgit2_init, plus the selected declaration inventory;
an empty umbrella result is a failed preflight. Save stdout, stderr, generated
Rust, and hashes for each engine. Repeat these
untimed calls to establish stable bytes independently for each engine. Before
using real headers, a small control must verify comment inclusion/removal, unsigned
macro type, enum prefix, and Debug generation in the exact executable. It must also
show that `capture` returns no `samples_ms` or `warmup_ms` fields.

Compare the generated files through the existing structural and native tool:

```console
python3 scripts/compare_bindings.py \
  --toucan-bindings zlib-toucan.rs --bindgen-bindings zlib-bindgen.rs \
  --target x86_64-unknown-linux-gnu --analyzer /path/to/toucan-binding-compare \
  --edition 2021 --output zlib-comparison.json
```

Rust 2021 admits the plain extern blocks required by the requested Rust 1.64
output target. The comparison tool otherwise defaults to Rust 2024.
Use explicit files, not `--benchmark-record`: the latter applies the legacy
macro/enum policy. Compile generated modules with current Rust and actual Rust
1.64, preserving their target guards. Reuse the project's C layout/value/FFI
checks with these exact outputs. Classify every structural difference and
unsupported probe; do not treat matching public names or record sizes as complete
API/ABI equivalence. A partial API or failed generation is not a speed result.

Create one new reference JSON per project containing:

- `request`: the exact new Builder request.
- `dependency_sha256`: the complete recorded input inventory.
- `observations`: `toucan-builder` and `bindgen` lists of untimed
  `{"output_sha256": "..."}` records.
- `configurations`: each engine's exact `configuration` object from capture stdout.
- The associated comparison, Rust compilation, and native-check artifacts.

Start timing after output validity and any stated limitations are recorded.

## Paired timing after the final freeze

The existing Python runner checks input hashes before generation, exact output
hashes and captured configuration after every process, and inputs/binary again
at the end. It refuses a Builder run without explicit roots and captured
configuration. Use a fresh output directory:

```console
taskset -c 3 python3 scripts/benchmark_inprocess.py \
  zlib.reference.json sqlite.reference.json zstd.reference.json libgit2.reference.json \
  --binary /path/to/toucan-inprocess-benchmark --toucan-engine toucan-builder \
  --pairs 7 --iterations 5 --seed 20260909 --output /tmp/builder-timing-final
```

Choose an available CPU after checking the shared host; CPU 3 is an example.
Record affinity, CPU model, kernel, load, frequency/governor information when
readable, environment, and concurrent work. Do not alter shared host settings.
This command makes seven separate process pairs per project and randomizes engine
order within each pair using the recorded seed. Each process discards one warmup
and then makes five measured calls. Project order is the supplied order; any
additional capture with a different project order is a separate recorded run.

Report raw samples, warmups, medians, pair order, and descriptive spread. For a
paired summary, first take each process's median and compare the two medians in
its pair; repeated calls in one process are not independent process samples.
Include all valid observations. Shared CPU frequency, memory bandwidth, file
cache, and other workloads limit small-change claims. Repeat a noisy result
under controlled conditions before calling it an improvement or regression.

The runner's `paired` object contains those per-process medians, paired ratios,
ranges, and separate first-call summaries. The top-level project `median_ms` and
`bindgen_over_toucan` fields retain their historical pooled-call calculation.
Use the paired object when comparing process pairs; its median ratio can differ
from the ratio of the two displayed medians.

The old core-route evidence remains a separate comparison. The new file roots,
Builder policies, comment work, and selected API change the measured workload;
a percentage change against old core results would conflate those changes.

## Consumer profiles

An optional zstd-sys or AWS-LC route must reproduce its existing build script's
headers, file roots, defines, blocklists, Rust target, derives, enum policy, and
callbacks. AWS-LC's prefix callback and Rust 1.70 requirement make it a distinct
profile. Reuse the pinned consumer harness and its captured generation command;
do not add an approximate AWS-LC path or build the entire consumer merely to
obtain a timing sample. Formatter-enabled consumer cost, if measured, must be
reported separately from this formatter-free frontend comparison.
