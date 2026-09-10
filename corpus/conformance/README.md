# External C acceptance audit

[`audit_c_testsuite.py`](../../scripts/audit_c_testsuite.py) compares Toucan with
GCC and Clang on the untouched sources in
[c-testsuite](https://github.com/c-testsuite/c-testsuite). The
[manifest](c-testsuite.json) pins the commit and archive SHA-256. This revision
contains 220 small programs, primarily imported from SCC and TinyCC.

This checks syntax and semantic acceptance. It does not link or execute these
programs, compare their expected output, validate ABI layout, or establish full
C conformance. The default route supplies compiler-preprocessed source. The
opt-in [native-source route](#native-source-route) additionally preprocesses and
analyzes the original files through Toucan. Neither route checks rejection of
invalid C. The separate [binding corpus](../README.md) checks layouts, constants,
and actual C/Rust calls.

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
GCC is the default. The selected preprocessor also selects Toucan's GNU or Clang
compiler profile. Use `--preprocessor clang` on Darwin and Windows, where the
frontend currently supports only Clang profiles. `--dialect gnu11` changes preprocessing and the comparison's
eligibility criterion from C11 to GNU11. All compiler modes below are still
recorded.

## Native-source route

Build the existing library audit probe alongside the CLI, then enable the route:

```console
cargo build --release --locked -p toucan_cli -p toucan --bin toucan --example audit_translation_unit
python3 scripts/audit_c_testsuite.py --native-source \
  --native-probe target/release/examples/audit_translation_unit \
  --preprocessor gcc --workers 4 --fail-on-strict-difference
```

Repeat with `--preprocessor clang` to exercise the Clang profile. The opt-in route
currently requires `--target` to match the native host. The selected compiler's
`-E -v` output supplies the ordered resource and system include directories. Explicit
`-I` arguments must name existing absolute directories. Ordered `-D` and `-U` options
from the common and selected-compiler argument lists are applied to both routes.
Other explicit compiler flags, quote-only search entries, and framework entries
fail configuration instead of being discarded. Cross-compilation, sysroot flags,
and other include-option forms remain available to the default compiler-preprocessed
route, but are not translated by this opt-in route.

Toucan retains its shipped target, compiler, and language-mode predefines and
feature-query behavior. The compiler's `-dM` output is recorded alongside Toucan's
configured definitions, including every missing or differing macro. The audit does
not install the compiler's definitions into Toucan or claim that both preprocessors
select identical conditional branches. Discovered include directories use the
probe's ordered `include_dirs`; compiler system-header warning metadata is not
reproduced. This tests original-source acceptance with explicit configurations,
not token-for-token preprocessing equivalence.

Each original file first runs through the probe's preprocessing operation, then
through its ordinary `parse_file` analysis with checked-code retention disabled.
The latter operation reads the original file again. Both operations report the
actual filesystem dependencies; compiler depfiles are not substituted for them.
The audit records complete native preprocessing output, declaration output, their
hashes, and the embedded resource-header configuration. It hashes every reported
file before and after analysis, compares the dependency sets, and rechecks all
sources and read headers after the complete run. Changed inputs fail the audit
and update both the final report and the affected case sidecars. An early
preprocessing rejection has no complete dependency inventory and never counts as
accepted native input.

The top-level counts continue to describe the compiler-preprocessed route.
`summary.native_source` separately records native acceptance, preprocessing and
analysis rejections, tool/protocol failures, changed inputs, and strict-positive
differences. `--fail-on-strict-difference` requires **both** routes to accept every
eligible pedantic-positive case when native processing is enabled. Exploratory
rejections stay visible without failing that narrower gate. Crashes, timeouts,
missing or malformed probe JSON, inconsistent status/exit codes, and changed
inputs always fail. Probe executable hashes and the checked-out protocol source
are recorded; a separate build record must establish executable source provenance.

The original source bytes and existing compiler-preprocessed route are preserved.
This does not add negative-source, ABI, code-generation, or runtime conformance
claims, and it does not establish complete standard-library header coverage.

## Method and evidence

For every original source, each compiler runs:

- C90, C99, C11, and C17 with `-fsyntax-only`.
- GNU90, GNU99, GNU11, and GNU17 with `-fsyntax-only`.
- `-std=c11 -pedantic-errors -fsyntax-only`.

Only the pedantic Clang classification adds `-Wno-strict-prototypes` and
`-Wno-deprecated-non-prototype`. These suppress deprecation diagnostics for
non-prototype declarations that remain permitted in C11. No diagnostic or source
filter hides a Toucan rejection.

The selected compiler then runs `-E -P` in the selected dialect. It checks the
resulting `.i` file again, and Toucan runs `check --target TARGET --compiler COMPILER` on that same file.
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
results. Every command has retained stdout and stderr. Child commands use `LC_ALL=C`
and `SOURCE_DATE_EPOCH=0`; the native library probe uses its deterministic Unix-epoch
timestamp default. Each completed case also has
its own `cases/NNNNN/result.json`, so partial diagnostics survive an interrupted run.
The recorded durations describe this audit; they are not performance benchmarks.

Ordinary acceptance differences do not make an exploratory audit fail. Use
`--fail-on-difference` to require that Toucan accept every eligible input. Tool
startup failures, crashes, timeouts, changed executables, and failure to preprocess
or recheck an otherwise accepted source always fail the audit. There is no changing
baseline count or exception list: every rejection remains visible. Reports apply
only to their recorded executables and flags.

## Strict acceptance gate

`--fail-on-strict-difference` requires Toucan to accept every eligible case that
both compilers also accept with the pedantic C11 flags above. The report includes
`strict_eligible_count`, `strict_toucan_accepted`, and `strict_differences` alongside
all exploratory differences. Infrastructure failures remain fatal under either
policy. There is no case exception list or fixed expected passing count.

```console
python3 scripts/audit_c_testsuite.py --workers 4 --fail-on-strict-difference
```

The [conformance workflow](../../.github/workflows/conformance.yml) runs this gate
on Ubuntu with Rust 1.96, Python 3.12, explicit GCC 13 and Clang 18 executables,
and four workers. Separate GCC and Clang jobs check both their compiler's
preprocessed input and the original-source route with the corresponding frontend
profile. It also tests the gate's failure behavior and retains reports, inputs,
and diagnostics as artifacts, including failed audits.

## Recorded results

The [native-source audit](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/native-conformance-9d8901b-2026-09-09/summary.json)
uses CLI and library-probe binaries built from `9d8901b`; all 630 recorded build
inputs match `66c87396`. Both GCC 13 and Clang 18 runs checked all 220 original
sources. The compiler-preprocessed and native-source routes each accepted 219
cases and all 211 pedantic-positive cases. Only the known `00144.c`
discarded-qualifier rejection remained, outside the strict subset. No native
preprocessing failures, tool/protocol failures, or changed inputs were recorded.

The [capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/native-conformance-9d8901b-2026-09-09/README.md) separates
build provenance, the uncommitted audit-driver hash, and each route's results.
It retains source/header hashes and diagnostics without redistributing downloaded
upstream sources or preprocessed files. The driver keeps compiler/Toucan macro
differences visible; these results do not imply identical preprocessing output.

The [CI confirmation](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/native-conformance-ci-2026-09-09/README.md)
repeats both profiles with Rust 1.96.0 release builds. Each route again accepts
all 211 pedantic-positive cases. The tested merge tree exactly matches PR head
`ca892441`; the supplement preserves build provenance and independently checked
reports without changing the local evidence above.

The [compiler-profile refresh](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/conformance-profiles-440db13/summary.json)
at `440db13` checks all 220 sources separately with GCC-preprocessed/GNU-profile
and Clang-preprocessed/Clang-profile inputs. Each passes all 211 pedantic-positive
cases, with no tool failures or unexplained strict rejections. Both retain
`00144.c` as the same discarded-qualifier difference described below. Compressed
reports preserve every case classification and input hash; the summary retains
that difference's compiler and frontend diagnostics. This refresh includes
Boolean macros, VLA type identities, and half types, and predates later changes.

The [recorded acceptance report](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/c-testsuite-2026-09-08.json) preserves
all 220 case classifications, source and preprocessed hashes, tool hashes,
configuration, and origin metadata from the existing audit. Toucan accepted
219 of 220 eligible C11 cases and all 211 cases accepted by both pedantic C11
oracles. Recomputing the strict gate from those retained results succeeds; requiring
all exploratory differences to disappear fails.

The remaining case, `00144.c`, assigns a conditional `const void *` result to
`void *`. Both pedantic compilers diagnose the discarded qualifier, as does
Toucan. Its [complete compiler and frontend diagnostics](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/c-testsuite-2026-09-08-differences.json)
remain recorded as an exploratory difference. This classification does not make
the other 211 programs a proof of full C conformance.

The separate [zstd feature report](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/zstd-features-2026-09-08.json) links
to eight complete native consumer reports: default, experimental, multithreaded,
and combined profiles, each built with Rust 1.64.0 and Rust 1.98.1. Upstream and
Toucan-generated bindings produced matching results and all 50 recorded runtime
artifacts; 72 generated layout tests passed. These are native x86-64 Linux consumer
checks, with each report retaining its actual frontend hash. They do not represent
full Ruff or uv builds.

The [size_t dependency regression](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/size-t-collection-2026-09-08.json)
records a later default/experimental consumer run: all 11 runtime artifacts match
and 18 generated layout tests pass. Both Darwin target profiles also generate
independent zstd/zdict files without the discarded `__darwin_size_t` alias. That
reproduction uses unchanged pinned zstd headers and a minimal synthetic sysroot;
native macOS SDK and runtime validation remains a separate CI check.

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
