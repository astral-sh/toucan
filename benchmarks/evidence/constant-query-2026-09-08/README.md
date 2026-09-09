# Constant-value query evaluation

Libgit2's enum aliases caused each binding generation to copy the complete
semantic environment 47 times. The evaluator now copies only the referenced
integer values when a bounded AST walk proves that an expression consists of
literals, known constants, and the existing arithmetic or conditional operators.

The change reduces libgit2 generation time from 285.329 to 218.466 ms in this
capture. All four project routes produce identical Rust output and binding
reports, apart from elapsed times. The source baseline is `a215c7e`; the initial
investigation of `a7838ce` is archived separately.

## Scope

Each query still validates the public translation unit, source, parser limits,
parameter contracts, and target profile. The proof keeps the existing depth and
4,096-node limits. It collects names from the parsed AST and copies each referenced
`IntegerValue`, including its width, signedness, and conversion rank, into a fresh
analyzer. These values contain no type-owner IDs. The original unit is unchanged.

Unknown names, casts, type queries, function calls, declarations inside expressions,
and exhausted proof budgets use the existing full-context path. Five libgit2
typedef aliases still fail through that path, with unchanged diagnostics. They
account for the ten remaining copies per generation because both integer and
arithmetic evaluation are attempted.

## Measurements

Median elapsed milliseconds per in-process generation, using the system allocator:

| Header | Baseline | Candidate | bindgen 0.72.1 | Candidate change |
| --- | ---: | ---: | ---: | ---: |
| zlib 1.3.1 | 33.210 | 33.507 | 118.930 | +0.9% |
| SQLite 3.45.1 | 51.625 | 52.034 | 154.641 | +0.8% |
| zstd 1.5.7 | 8.838 | 8.886 | 100.890 | +0.5% |
| libgit2 1.9.1 | 285.329 | 218.466 | 259.455 | -23.4% |

The seven rounds randomize project and engine order. Each process discards one
warmup and makes five timed calls: 84 processes and 420 timed calls. The calls
include configuration, preprocessing, parsing, semantic analysis, and Rust string
generation. Process startup, request decoding, and first libclang initialization
are excluded. CPU affinity is 30 on the shared AMD EPYC-Milan virtual machine;
frequency, memory bandwidth, and other processes are uncontrolled.

The libgit2 improvement is large in this capture; the other routes' medians are
0.5–0.9% higher. Those small differences need an isolated machine to assess. No
unchanged-latency claim is made for those routes. Raw samples, each round's order,
and descriptive bootstrap intervals over paired round medians are preserved.
The intervals do not account for uncontrolled host conditions.

Separate allocation counters delegate every operation to `std::alloc::System`.
They count allocation/reallocation requests and sum requested sizes, including
the full new size of each reallocation. They do not measure live heap or peak RSS,
and they exclude native libclang allocations. Their timings are not used above.

| Libgit2 phase | Requests before | Requests after | Requested bytes before | Requested bytes after |
| --- | ---: | ---: | ---: | ---: |
| Configuration | 449 | 449 | 21,106 | 21,106 |
| Parsing and analysis | 490,051 | 490,051 | 141,879,337 | 141,879,337 |
| Binding generation | 894,071 | 216,193 | 60,620,959 | 17,744,764 |
| Total | 1,384,571 | 706,693 | 202,521,402 | 159,645,207 |

The allocation capture contains three randomized process pairs per project and
five generations per process, after a warmup: 120 measured generations. All
allocation counts and requested bytes are unchanged on zlib, SQLite, and zstd.

Callgrind verifies the path change on the exact release executables. Across one
warmup and three generations, full-unit copies fall from 228 to 40. Executed
instructions fall from 7,578,004,370 to 6,056,693,536. Instruction counts are
separate from native elapsed-time measurements. The profiles include startup and
teardown; they are not estimates of isolated function latency.

The original investigation records all 52 expressions that used full integer-query
contexts: 45 enumerator aliases, two bitwise combinations, and five rejected
typedef aliases. It also retains per-query allocation counts and results. The
original and matched profiles identify preprocessing normalization and lexing as
other substantial costs; this change does not modify those paths.

## Validation and limits

- The workspace suite passes 832 tests, with 222 ignored. Workspace Clippy with all features, fuzz-workspace Clippy, and formatting
  checks pass.
- Six enum tests pass with native GCC and Clang checks. The enum matrix now also
  checks that direct queries preserve the final constant's exact integer type,
  including the different GNU and Clang rules after an enum definition.
- Two standalone before/after comparisons cover all 88 compiler/target/language
  settings. They compare 54,800 expressions through integer, arithmetic, and
  vector query APIs: 164,400 identical results, including errors and offsets.
  Ninety-six profile/case pairs reject their source unit identically; the separate
  portable enum case parses in all 88 settings.
- Cases include signed and unsigned wide values, ordinary-name and builtin-name
  collisions in public unit state, file/block enum shadowing, local query types,
  malformed public metadata, unknown names, and exhausted proof budgets.
- Every four-project run matches its engine's prior independently validated
  output. Complete before/after Toucan binding reports match after removing only
  the elapsed-time field. This does not claim complete API or byte equality
  between Toucan and bindgen; the existing C-validated differences are described
  in [the benchmark report](../../../docs/benchmarks.md).
- A fresh AddressSanitizer bindings smoke run executes 4,945 inputs in 80.88 seconds,
  adds 68 corpus inputs, and reports a 604 MiB peak RSS, with no failure artifacts.
  The new constant-query seed covers all 88 selector settings. The run is configured
  for 60 seconds; its elapsed duration includes startup and initial corpus work.
  LeakSanitizer is disabled in this environment. This short run is not broad
  input-space coverage.

The source stays unchanged throughout each capture. Baseline and candidate use
separate source and target directories. Executable symbols distinguish the old
literal-only proof from the new value proof before timing. Generated output is
checked before and during timing. The records contain hashes for all 149 actual
header inputs, the loaded libclang 18.1.3 library, the executables, source, helper
programs, and requests. Inputs and binaries are checked again at artifact freeze.

## Artifacts

`summary.json` records the results and artifact hashes. The compressed JSON files
preserve timing, allocation, output, input, source, query, profile, and sanitizer
records. `seed-coverage.json` records each of the 88 padded seed inputs.
`timing-source.patch.gz` is the exact implementation/test diff used for the release
measurements; the sanitizer seed was added afterward without changing that source.

`capture-artifacts.tar.gz` contains the original capture runners, request files,
query inputs and outputs, generated Rust, complete binding reports, preprocessed
text, helper source/manifests/locks, build and validation logs, raw Callgrind data,
and the sanitizer starting corpus and runner. `capture-artifacts.json` lists each
member's size and hash. The earlier `a7838ce` investigation is under its own prefix
inside the archive. Executables remain in the recorded local cache; the archive
contains their reproducible source and recorded hashes.

To reproduce, check out `a215c7e` twice, decompress and apply `timing-source.patch.gz` to the candidate,
and build the archived in-process and helper drivers against their respective
copies, using separate `CARGO_TARGET_DIR` values. The archived helper manifests
and runners preserve the original absolute cache paths; adjust those paths if
replaying elsewhere. Recreate the requests' headers, include paths, target,
sysroot, and allowlists with the recorded input hashes. Run output and query
comparisons before timing, then collect a new capture in fresh directories.
Preserve this capture unchanged.

## Stack integration

The [integration record](root-integration.json) checks this change on top of
`f9d6f95`. The measured production sources match exactly; the intervening source
changes fix native test linkage. All 832 workspace tests, ten focused query and
native enum tests, and workspace Clippy pass. Eight header outputs and four musl
outputs remain byte-identical. `root-integration.tar.gz` preserves the integration
commands, logs, and output hashes. No new timing or macOS run is included.
