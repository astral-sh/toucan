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

## Historical results

The [validation archive](../docs/validation.md#historical-results) preserves earlier
native runs. Current workflow artifacts report the actual revision under test.
Commit reusable probe inputs here; keep generated bindings and logs in ignored
`corpus/results` output or CI artifacts.
