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

## Recorded native run

The [native run on 2026-09-08](https://github.com/astral-sh/toucan/actions/runs/34174435203)
passed on x86_64 and AArch64 Linux and macOS. Each target compiled all four libraries and
completed their FFI checks. Each compared 1,319 integer constants, seven strings, and 17
records with 88 field offsets, for 4,086 checks. Each generated 1,648 declarations and
skipped no selected declarations.

| Targets | C compiler | Rust | Omitted macros |
| --- | --- | --- | --- |
| x86_64 and AArch64 Linux | GCC 13.3.0 | 1.96.0 | 111 |
| x86_64 and AArch64 macOS | Apple Clang 17.0.0 | 1.96.0 | 110 |

The [compact evidence](evidence/native-2026-09-08.json) preserves the tested checkout and PR
head commits, project versions, per-target counts, executable and binding checksums, and
artifact identifiers and digests. The workflow artifacts retain the complete reports, probe
sources, commands, and logs for 14 days. The omitted macros include decoration macros,
function-like macros, aggregate initializers, and SQLite's destructor sentinels. Linux adds
the empty `Z_LFS64` feature macro to the omitted set. Windows was not run.

This historical run predates enum constant projection, prototype-scope changes, bindgen API
comparison, and the all-record layout probes. It establishes the recorded C and FFI checks
for its tested version. Results for later changes and broader comparisons are separate. The
[earlier local Linux result](evidence/linux-x86_64.json) is retained as a separate snapshot.

## What is checked

- All selected public API prefixes and their transitive type dependencies are generated.
  The manifest also names zlib entry points such as `compress2` and `inflate` that do not start
  with `z`. Selection does not reduce the header text passed to the frontend.
- Generated bindings compile with `improper_ctypes` and `improper_ctypes_definitions` denied.
- Every emitted integer constant is compared with the C compiler for value, width, and
  signedness. Every emitted byte-string constant is compared including its terminator.
- C `sizeof`, `_Alignof`, and `offsetof` results are compared with Rust for the public records
  listed in [probes.json](probes.json).
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
