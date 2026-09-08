# Upstream compatibility corpus

The corpus builds pinned releases of zlib, SQLite, zstd, and libgit2, generates bindings from
their public headers, and compares the result with separately compiled C probes. Headers are
not edited or replaced with reduced fixtures.

## Run

Prerequisites: Python 3.12+, a C compiler, Rust, CMake, Make, Tcl, `ar`, and GitHub CLI.
Downloads use `gh-auto` when installed so account routing follows the checkout configuration,
and otherwise use `gh`. `--gh PATH` selects the executable explicitly.

```console
python3 scripts/prepare_corpus.py
cargo build -p toucan_cli
python3 scripts/verify_corpus.py --sysroot /
```

On macOS, pass the SDK explicitly:

```console
python3 scripts/verify_corpus.py --sysroot "$(xcrun --show-sdk-path)"
```

`--cache PATH` selects another archive/source/build directory. Both scripts accept repeatable
`--project zlib|sqlite|zstd|libgit2` arguments. `prepare_corpus.py --offline` requires previously
downloaded archives and still verifies every archive checksum. `--jobs` controls build jobs;
`--workers` controls how many projects are built at once.

Build output lives under `corpus/cache`; verification artifacts live under `corpus/results`.
Both directories are ignored by Git. A failed command returns a nonzero exit code and retains
its stdout, stderr, and invocation. `evidence.json` reports the status of every requested
project and names any missing builds.

## Recorded native results

The [native run on 2026-09-08](https://github.com/astral-sh/toucan/actions/runs/34216957161)
passed C/FFI probes, bindgen API comparison, and independent record-layout probes on x86_64
and AArch64 Linux and macOS. Each target built all four pinned libraries and generated 1,648
declarations, with no selected declarations skipped.

Each target passed 5,444 C/Rust comparisons covering 1,319 integer constants, seven strings,
and the 17 records and 88 offsets selected in the FFI corpus. These include separate checks
of the C expression types and Rust enum representations of 679 enumerators. Actual calls
exercise all four libraries.

Bindgen comparison found matching signatures for 1,284 functions and matching types for
three globals on every target. The table records the remaining target-dependent coverage:

| Target | Shared typedefs | Complete records checked against C | C field offsets | Shared field offsets compared with bindgen |
| --- | ---: | ---: | ---: | ---: |
| `x86_64-unknown-linux-gnu` | 203 | 111 | 636 | 636 |
| `aarch64-unknown-linux-gnu` | 207 | 111 | 638 | 628 |
| `x86_64-apple-darwin` | 204 | 111 | 636 | 636 |
| `aarch64-apple-darwin` | 208 | 109 | 628 | 628 |

Counts are per project, so a helper type used by two projects can appear twice. AArch64
Linux's `va_list` has named fields in Toucan and private storage in bindgen. Its fields are
checked against C separately; they are excluded from the shared-field comparison. AArch64
macOS represents `va_list` as a pointer and therefore has fewer record types.

All targets passed with no unexplained differences. Exact API equivalence remains false:
the reports retain specific constant-type and sentinel differences, SQLite's `xDlSym`
callback discrepancy, extra Toucan constants, helper-name differences, and private storage
representations. The type, value, and callback exceptions have separately compiled C oracles.

Linux used GCC 13.3.0; macOS used Apple Clang 17.0.0. Both used Rust 1.96.0 and bindgen
0.72.1. Reports name 111 omitted macros on Linux and 110 on macOS, including decoration
macros, function-like macros, aggregate initializers, and SQLite's destructor sentinels.
Linux additionally defines the empty `Z_LFS64` feature macro. Windows was not run.

The [compact evidence](evidence/native-06cefbe/summary.json) records `06cefbe` and its
CI merge commit, whose Git tree is identical. It preserves each scope's counts,
accepted differences, executable and generated-source hashes, and artifact metadata.
Compressed original reports are checked in beside the summary. Workflow artifacts
retain probe sources and command logs for 14 days.

The same four-target run passed all four zstd feature profiles with Rust 1.96 and
1.64, the SQLite Rust consumer, and Rust 1.64 record-layout checks. Runtime output
artifacts were byte-identical to the upstream zstd wrapper's output. The Linux
source audit accepted all seven translation units through Toucan preprocessing;
x86-64 also accepted all seven compiler-preprocessed inputs, while AArch64 retained
two compiler-preprocessed rejections. The summary keeps these scope differences.

This source predates packed enums and derived `__auto_type` declarators. Its
separate native workspace jobs failed on the recorded Apple atomic-pointer cast
crash; the passing corpus does not imply those jobs passed. See
[compiler oracle discrepancies](../docs/compiler-oracle-discrepancies.md).

A [separate native test run](https://github.com/astral-sh/toucan/actions/runs/34177031459)
passed optimized C/Rust `va_list` calls on all four targets, along with the workspace tests,
lint, and Rust 1.96 checks. Its commit and checkout provenance are recorded separately in
the [earlier summary](evidence/native-equivalence-2026-09-08.json). The [CI fuzz smoke run](https://github.com/astral-sh/toucan/actions/runs/34177031461)
passed with the default sanitizer configuration; [local fuzz runs](../fuzz/README.md) have
separate scope and sanitizer settings.

The [earlier four-target result](evidence/native-2026-09-08.json) and
[earlier local Linux result](evidence/linux-x86_64.json) remain historical snapshots. The
newer record-layout and API checks overlap the FFI corpus; their counts are not a count of
additional unique assertions.

## What is checked

- All selected public API prefixes and their transitive type dependencies are generated.
  The manifest also names zlib entry points such as `compress2` and `inflate` that do not start
  with `z`. Selection does not reduce the header text passed to the frontend.
- Generated bindings compile with `improper_ctypes` and `improper_ctypes_definitions` denied.
- Every emitted integer constant is compared with the C compiler for value, width, and
  signedness. Enumerators are checked both as C expressions and as values represented by
  their enum type. Every emitted byte-string constant is compared including its terminator.
- C `sizeof`, `_Alignof`, and `offsetof` results are compared with Rust for the public records
  listed in [probes.json](probes.json).
- The differential tools compare generated APIs with bindgen, then independently compile
  C and Rust probes for every complete record and ordinary field in Toucan's output. See
  the [comparison tools](../tools/binding_compare/README.md) for commands and scope.
- The Rust program links the freshly built static library and calls its actual implementation:
  zlib and zstd compression round trips, SQLite in-memory prepare/bind/step/finalize, and
  libgit2 initialization, version queries, object ID conversion, and signature allocation.
- libgit2's `git_commit_create_options` bitfield is read and written in both directions:
  a separately compiled C helper reads values written by Rust and writes values read by Rust.
  The ordinary fields around the bitfield are checked for size, alignment, and offset too.

The C probe includes the original public header directly. It does not use Toucan's reported
layout or constant values when constructing expected results. Probe names are shared; C and
Rust compute the results independently.

Nonconstant and unsupported macros are retained in the binding report. A successful corpus
run proves the emitted declarations and tested calls for that configuration; it does not
claim support for every macro or API behavior. Native execution is recorded for the actual
host only. The target crate's cross-compilation probes provide separate layout evidence.

## Source and build provenance

[manifest.json](manifest.json) pins release versions, resolved commits, archive references, and
archive SHA256 values. zlib and zstd use annotated tag objects as their archive references;
their resolved commits are recorded separately. Archives are verified before extraction, and
the extractor rejects unexpected roots and uses Python's safe data extraction filter.

`prepared.json` records commands, logs, host compiler, and static-library checksums. SQLite's
official build tools generate its amalgamation and public header from the pinned source tree.
The SQLite test library disables threading; the libgit2 test library disables SSH, HTTPS,
NTLM, and iconv, and uses its bundled zlib and regex engine. These are library build options;
the upstream public headers remain intact.

The verification report records the frontend executable checksum, target, host, compiler
versions, source archive checksums, header checksums, binding checksums, selected declaration
counts, skipped macros, probe coverage, timings, and every command. Timings describe individual
verification runs; they are not a comparative benchmark.

## Complete C sources

The [translation-unit audit](translation-units.md) checks seven pinned, untouched
C sources with their actual build flags. It compares normal and retained analysis
through Toucan preprocessing and unchanged compiler-preprocessed input. Linux CI
requires all seven Toucan-route inputs to pass; compiler-route gaps remain explicit
in the report.
