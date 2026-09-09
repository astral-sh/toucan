# Native preprocessing acceptance at `9d8901b`

The original-source route and the existing compiler-preprocessed route both pass
the pedantic-positive gate on x86-64 GNU/Linux:

| Compiler/profile | Original sources | Compiler-preprocessed accepted | Native-source accepted | Pedantic-positive accepted, each route |
| --- | ---: | ---: | ---: | ---: |
| GCC 13.3 / GNU | 220 | 219 | 219 | 211 / 211 |
| Clang 18.1.3 / Clang | 220 | 219 | 219 | 211 / 211 |

Both process exits were zero. The only exploratory rejection is `00144.c`, whose
assignment discards a pointer qualifier. Both pedantic compilers reject that case,
so it is outside the strict-positive subset. Every native preprocessing operation
succeeded. No tool/protocol failures, changed executables, changed source/header
bytes, or strict differences were recorded.

Toucan read 45 distinct filesystem headers in the GCC configuration and 53 in the
Clang configuration, in addition to each run's 220 original sources. The library
probe reports its actual dependencies after preprocessing and after original-file
analysis; their sets and hashes agree. The audit rechecks every input at completion.
Its captured embedded resource headers belong to Toucan.

## Configuration and scope

The native route retains Toucan's shipped target/compiler/language profile. It
uses the selected compiler's discovered include order and no additional `-D` or
`-U` arguments in these runs. GCC has 249 compiler-only macros, seven Toucan-only
macros, and 41 differing replacement values. Clang has 221 compiler-only macros,
five Toucan-only macros, and no differing shared replacements. Those differences
are retained in full; no compiler macro dump is installed into Toucan.

The run uses C11, native `x86_64-unknown-linux-gnu`, two workers per compiler
profile, and a ten-second per-command timeout. The compiler and CLI receive
`LC_ALL=C` and `SOURCE_DATE_EPOCH=0`; the library probe uses its deterministic
Unix-epoch default. Checked-code retention is disabled. The native probe uses a
2,000,000-token preprocessing budget and its existing resource limits.

These are positive acceptance results with explicit configurations. They do not
establish identical preprocessing output or conditional branches, invalid-source
rejection conformance, runtime behavior, typed-operation equivalence, ABI
compatibility, or full C conformance. Compiler system-header warning metadata is
not reproduced by the probe's ordered `include_dirs`. No macOS, Windows, consumer,
or cross-target run is included. Recorded durations are audit observations, not
performance benchmarks.

## Build and driver provenance

The CLI and existing `audit_translation_unit` example were built together from
`9d8901b` using `cargo +ohm build --locked`, without workspace tests or a release
build. All 630 recorded build inputs remained unchanged and match the `66c87396`,
`85bf1ad`, and `9d8901b` Git blobs. The build uses local Ohm experimental defaults
and trusted-cache opt-ins, recorded alongside the exact toolchain and executable
hashes in [build.json.gz](build.json.gz). This is separate from stable-toolchain CI
and production release evidence.

The Python audit driver was uncommitted during these runs. Its exact hash is
`fed0ebfa288b752db7aa739ea244533bc42e3c3f1dc32520832938e78ca2832c`.
The capture preserves that driver, its 14 passing gate/protocol tests, the existing
probe source, and the corpus manifest. [summary.json](summary.json) separates the
frontend build identity from the candidate driver and gives exact audit commands.

## Evidence and independent review

[gcc.json.gz](gcc.json.gz) and [clang.json.gz](clang.json.gz) preserve the complete
raw reports. [capture.json.gz](capture.json.gz) contains 21,670 named text
artifacts stored as 2,417 unique contents: command logs, JSON requests, per-case
results, configuration, and owned tooling source. Downloaded upstream source
and header bytes, compiler/native `.i` text, and declaration-output bytes are
excluded. Their hashes remain recorded; the original local reports retain them.
The pinned archive, source URLs, commands and compiler identities support
reproduction without adding those third-party source files to this repository.

The [independent review](independent-review.json) checked 10,170 command records,
20,340 logs, all 880 case-probe requests/results, all 440 source copies against the
pinned archive, and all 630 build inputs against three Git revisions. It also
verified every retained artifact against its original bytes and all 3,140 excluded
artifact hashes. It found no material false-pass issue. The archived reviewer
scripts use the original local paths; they do not rerun compilers.

Verify the portable package, recompute strict membership, and check both recorded
gates without compiling or accessing downloaded inputs:

```console
python3 -B corpus/evidence/native-conformance-9d8901b-2026-09-09/verify.py
```

This verifies captured consistency; it does not recreate the compiler executions.
To reproduce the audit, follow [the native-source route](../../conformance/README.md#native-source-route)
and the exact commands in the summary, using new output directories and recording
any relocated headers or different compiler versions.
