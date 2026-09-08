# External C acceptance audit

[`audit_c_testsuite.py`](../../scripts/audit_c_testsuite.py) compares Toucan with
GCC and Clang on the untouched sources in
[c-testsuite](https://github.com/c-testsuite/c-testsuite). The
[manifest](c-testsuite.json) pins the commit and archive SHA-256. This revision
contains 220 small programs, primarily imported from SCC and TinyCC.

This checks syntax and semantic acceptance. It does not link or execute these
programs, compare their expected output, validate ABI layout, or establish full
C conformance. Toucan receives compiler-preprocessed source, so the audit also
does not assess its native preprocessor. The separate [binding corpus](../README.md)
checks layouts, constants, and actual C/Rust calls.

## Run

Requirements: Python 3.12+, GNU GCC, Clang, and a built Toucan executable.

```console
cargo build --release -p toucan_cli
python3 scripts/audit_c_testsuite.py
```

Downloads go into `corpus/cache/conformance`. Each audit creates a fresh timestamped
report below that cache. The archive checksum is verified on every run, including
offline runs. Sources are freshly extracted into each report to avoid reusing an
edited working copy. Original sources and their license notices remain in the
ignored output directory; none are vendored in the repository.

For a quick check or a particular executable:

```console
python3 scripts/audit_c_testsuite.py --limit 3
python3 scripts/audit_c_testsuite.py --offline --case 00162 --case 00207 --toucan target/debug/toucan
```

`--cache` selects the archive cache. `--output` selects a new or empty report
directory; existing reports are never overwritten. `--workers` controls concurrent
source cases and `--timeout` bounds each compiler or Toucan command. A timeout
terminates the process group on POSIX and attempts to terminate the process tree
on Windows. Sources are never executed.

`--gcc` and `--clang` select the compiler executables. On macOS, select the installed
GNU GCC explicitly with `--gcc` or `TOUCAN_GCC`; `/usr/bin/gcc` is Apple Clang and is
rejected as the GCC oracle. Repeat `--cc-arg=VALUE` for flags shared by both
compilers, or use `--gcc-arg=VALUE` and `--clang-arg=VALUE`. These flags can provide
sysroots, include paths, defines, or architecture selection. They are recorded in
the report and checked with a small valid C control before the audit starts.

`--target` sets Toucan's target, defaulting to the native host. It does not change
the compiler targets: provide matching compiler executables and arguments when
cross-compiling. Use `--preprocessor clang` to check Clang's preprocessed output;
GCC is the default. `--dialect gnu11` changes preprocessing and the comparison's
eligibility criterion from C11 to GNU11. All three compiler modes below are still
recorded.

## Method and evidence

For every original source, each compiler runs:

- `-std=c11 -fsyntax-only`
- `-std=gnu11 -fsyntax-only`
- `-std=c11 -pedantic-errors -fsyntax-only`

Only the pedantic Clang classification adds `-Wno-strict-prototypes` and
`-Wno-deprecated-non-prototype`. These suppress deprecation diagnostics for
non-prototype declarations that remain permitted in C11. No diagnostic or source
filter hides a Toucan rejection.

The selected compiler then runs `-E -P` in the selected dialect. It checks the
resulting `.i` file again, and Toucan runs `check --target TARGET` on that same file.
A comparison is **eligible** when both compilers accept the original source in the
selected dialect and the selected compiler accepts its preprocessed output. An
eligible Toucan rejection is reported as a **difference**, without deciding
whether it is a Toucan bug, an unsupported extension, or a correctly rejected C
constraint violation that the compilers accepted with a warning.

Pedantic acceptance is useful evidence, but it does not certify strictly conforming
C: compilers still accept some extensions with those flags. Conversely, optional
C11 features can be valid without being supported by every implementation. Inspect
the source and diagnostics before classifying a difference.

`evidence.json` records the manifest, executable hashes before the run, checks that
they did not change during the run, compiler versions, relevant include environment,
arguments, source and preprocessed hashes, origin sidecars, timings, and acceptance
results. Every command has retained stdout and stderr. Each completed case also has
its own `cases/NNNNN/result.json`, so partial diagnostics survive an interrupted run.
The recorded durations describe this audit; they are not performance benchmarks.

Ordinary acceptance differences do not make an exploratory audit fail. Use
`--fail-on-difference` to require that Toucan accept every eligible input. Tool
startup failures, crashes, timeouts, changed executables, and failure to preprocess
or recheck an otherwise accepted source always fail the audit. There is no changing
baseline count or exception list: every rejection remains visible. Reports apply
only to their recorded executables and flags.

## Origins and licenses

The c-testsuite harness is MIT licensed. Its
[`tests/LICENSE`](https://github.com/c-testsuite/c-testsuite/blob/5c7275656d751de0e68b2d340a95b5681858ed07/tests/LICENSE)
directs users to individual `.otags` origin records instead of applying the harness
license to all fixtures. The manifest records those upstream repositories, pinned
revisions, and repository license files. SCC's recorded license is ISC; TinyCC's
recorded `COPYING` is GNU LGPL version 2.1. These repository-level files do not
establish the complete redistribution history of every imported fixture.

`00001.c` has no origin sidecar. The audit preserves missing origins as missing and
copies each available `.tags` and `.otags` record into the evidence. This runner
and manifest do not relicense the downloaded sources or claim a complete per-file
license audit.
