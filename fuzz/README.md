# Fuzzing

Install `cargo-fuzz` and a nightly Rust toolchain, then run a target from the repository root:

```console
cargo +nightly fuzz run preprocess -- -max_total_time=60
cargo +nightly fuzz run semantic -- -max_total_time=60
cargo +nightly fuzz run bindings -- -max_total_time=60
cargo +nightly fuzz run checked -- -max_total_time=60
```

Targets exercise UTF-8 input, bounded preprocessing, declaration analysis, layout,
and binding generation. Semantic, binding, and checked-code targets select among
all eleven compiler profiles using the sum of input bytes modulo eleven. The
`CompilerProfile::ALL` order preserves the original seven entries (five original
target defaults, then Clang on GNU Linux x86-64 and AArch64), followed by GCC on
musl x86-64 and AArch64, then Clang on those two musl targets. Preprocessing has no profile
selector. Archived campaigns using modulo five or seven retain their selector contracts;
replaying their exact bytes with the new harness can select a different profile.
Record the harness source and selector count with each campaign. The same byte sum's
bits 8 through 10 select GNU11, C11, GNU90, C90, GNU99, C99, GNU17, or C17:
`(sum(input bytes) >> 8) & 7` indexes that order. This is language selector
version 3. Older two-mode and four-mode campaigns retain their original contract; use their saved binary to reproduce it. No input
prefix is consumed. The preprocessing
harness uses bit 8 to disable or enable trigraph replacement, and bit 0 selects
GNU (clear) or Clang (set) feature-query argument rules with a small test catalog.
Preprocessing selector version 2 also uses `(sum(input bytes) >> 9) % 5` for
line-comment handling: enabled, GCC C90 compilation, GCC C90 preprocessing,
Clang C90 compilation, or Clang C90 preprocessing, in that order. The source
bytes remain intact. The runner appends block-comment padding to cover all 480
comment/query/trigraph/scope/macro-history/redefinition/documentation combinations for every preprocessing seed. Older
preprocessing campaigns always enabled line comments; replay their saved binary
to preserve that behavior. Its selectors are independent of the semantic targets'
eight-mode selector. Scope-punctuator tokenization is selected independently by
`sum(input bytes) & 2`; preprocessing selector version 3 records this addition.
Version 4 adds `sum(input bytes) & 4` to enable or disable macro-definition history.
The target validates captured definition locations and keeps the final environment
checks active in both modes. Version 5 adds `sum(input bytes) & 8` to select
strict redefinition errors or recorded incompatible replacements. The target
checks retained redefinition locations and resets the same preprocessor after
both success and failure. Padding preserves every original source byte.
Version 6 adds `(sum(input bytes) >> 4) & 3` for documentation capture: zero
disables capture, one retains documentation markers, and two or three retain all
comments. The runner covers the three distinct configurations. Source, comment,
and output-token coordinates are checked when a catalog is produced.
It has no physical target profile. The campaign runner pads each seed with a comment to cover every compiler
profile in all eight language modes, including all independent preprocessing settings. The
reported profile count must match the compiled harness's `CompilerProfile::ALL`.
The binding harness keeps its default generation pass and also requests Rust
enums with trait selector version 1: `(sum(input bytes) >> 11) & 15` controls
Copy, Debug, Default, and Eq with bits 0, 1, 2, and 3, respectively. This does not
consume or rewrite source bytes. Ordinary campaign padding still covers the
compiler/language pairs; the focused derive-storage replay additionally covers
all 1,408 compiler/language/trait combinations for that fixture.
The second generation also uses enum selector version 1: byte-sum bit 2 selects
bindgen enum naming and bit 3 selects enum-name prefixes. This exercises lexical
record ownership, shared anonymous numbering, and Rust-name collisions. The
default generation remains active for every accepted input.
Older archived sources retain their recorded selector contracts. The `checked`
target compares analysis with and without retained code. Successful results must have
identical declarations; invalid inputs must produce the same diagnostic, except
when the separate retention limits are reached. It also exercises layout queries
on the retained analysis owner. All targets reject invalid UTF-8, so saved reproducer
bytes are exactly the source used for preprocessing, analysis, binding generation,
and target selection.
Invalid input may return a diagnostic; panics, aborts,
timeouts, and sanitizer failures are findings. Minimize failures and add regression
tests before fixing them. Short local runs are smoke tests, not a completed fuzz campaign.

CI uses the C dictionary and immediately permits each harness's maximum input size:
16 KiB for preprocessing and analysis, 8 KiB for binding generation. The smoke
runs still last 60 seconds per target; they are not sustained campaigns.

The daily and manually dispatched workflow runs each target for 15 minutes with
AddressSanitizer. Successful default-branch runs save the evolving corpus for the
next campaign. Every run uploads its starting corpus archive, final corpus,
dictionary, binary, source hashes, logs, and reproducer files for 30 days. The
starting archive makes a recorded random seed useful even after the corpus changes.
These jobs start running after the workflow reaches the default branch; adding
the workflow is not evidence that a scheduled campaign has completed.

The same runner works locally with a nightly toolchain and `cargo-fuzz` selected:

```console
python3 scripts/run_fuzz_campaign.py checked --seconds 900 --seed 12345 --output fuzz/runs/checked-local
```

The output directory must be new. To replay a saved campaign, extract its starting
corpus into a new directory and invoke its saved binary with that directory and
the recorded libFuzzer arguments, updating the dictionary and artifact paths.
The binary requires a compatible host. Preserve the archive before further
mutations; a final corpus is not an exact substitute for the starting corpus.

The preprocessing targets disable filesystem access. Includes can resolve only to
the configured in-memory resource headers.

## Recorded smoke tests

The [exact-input profile smoke](evidence/input-profiles-2026-09-08.json) validates
all four byte-input harnesses at `48b7d73`, after atomic types and parser
limits. The 31-second AddressSanitizer runs processed 139,050 preprocessing inputs,
12,910 semantic inputs, 5,096 binding inputs, and 6,782 checked-code inputs without
findings. Seeds cover every target profile for the three target-aware harnesses.
Source and binary hashes are recorded; LeakSanitizer was unavailable under ptrace.
These runs validate the changed harnesses and do not replace sustained campaigns.

The [local evidence](evidence.json) records 382,661 preprocessing inputs, 446,536
semantic inputs, and 25,192 binding inputs, with no findings in the final 61-second
runs. An earlier semantic run found a parser timeout; the declaration analyzer now
rejects the pathological prefix run and has a regression test for it.

A [later semantic smoke run](evidence/semantic-2026-09-08.json) processed 1,019,653
inputs in 181 seconds with no findings. Its report records the tested source and
binary hashes, seed, resource limits, and final libFuzzer statistics. The default
maximum mutation length was 4,096 bytes.

AddressSanitizer was enabled. LeakSanitizer was disabled for these local runs because
it cannot inspect processes in the development environment's ptrace sandbox. CI
runs the default sanitizer configuration. These small runs leave substantial work
for sustained fuzzing and independent review.

## Body checking and resource limits

The [body-checking campaign](evidence/readiness-2026-09-08.json) records three
301-second runs after the record traversal and parser-chain fixes:

| Target | Fuzzer inputs | Maximum input size | Peak RSS |
| --- | ---: | ---: | ---: |
| Preprocessor | 641,846 | 16 KiB | 525 MiB |
| Semantics and layout | 588,980 | 16 KiB | 513 MiB |
| Bindings | 50,858 | 8 KiB | 514 MiB |

All three runs completed without sanitizer findings, timeouts, or memory-limit
failures. The older typed `&str` harnesses can analyze a valid prefix before an invalid
UTF-8 byte; these counts do not measure accepted programs or output equivalence. The harnesses use the x86_64 Linux target
and the binding harness uses default options. New seeds cover GNU statement
expressions, variadic bodies, variable arrays, inline assembly, sparse
initializers, repeated record members, and control flow. The shared `c.dict`
provides C and preprocessor syntax for mutations. All semantic and binding seeds
were separately checked for frontend acceptance.

The accompanying boundedness review found two defects that mutation limits alone
would not reliably reach: repeated record-member traversal made a 981-byte
assignment exceed the five-second timeout, and 94–105 KiB label/statement chains
overflowed the parser stack. The report preserves reproducer hashes, before/after
results, compiler and binary hashes, source hashes, seeds, commands, and limits.
The fixes also passed 5,444 native corpus comparisons with unchanged Rust output.

AddressSanitizer was enabled. A direct LeakSanitizer probe failed because process
inspection is unavailable under ptrace; leak checking remained disabled. These
bounded local runs complement the compiler differential tests and native FFI
checks. They do not establish complete safety or frontend conformance.

To reproduce a target with the checked-in seeds and dictionary:

```console
mkdir -p /tmp/toucan-fuzz-semantic
cp fuzz/seeds/semantic/*.h /tmp/toucan-fuzz-semantic/
cargo +nightly fuzz run semantic /tmp/toucan-fuzz-semantic -- -dict=fuzz/c.dict -max_total_time=300 -max_len=16384 -len_control=0 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
```

## Compiler-profile sanitizer campaigns

The [seven-profile campaign](evidence/2026-09-08-1d8f508/summary.json) at `1d8f508`
completed 2,318,467 executions with AddressSanitizer and no findings:

| Target | Executions | Duration | Peak RSS |
| --- | ---: | ---: | ---: |
| Preprocessor | 1,430,525 | 901 s | 517 MiB |
| Semantics and layout | 441,788 | 901 s | 513 MiB |
| Bindings | 163,821 | 901 s | 522 MiB |
| Retained-code comparison | 282,333 | 901 s | 513 MiB |

These sources include explicit compiler profiles, atomic Rust storage and call
validation, VLA identities, half types, and `nodebug`. They precede packed enums
and multiple/derived `__auto_type` declarations. The three target-aware harnesses
select all seven profiles; preprocessing has no profile selector.

The source manifest, saved binaries, logs, dictionary, starting corpus archives,
and every archived input were independently checked against their hashes. Reports
and starting archives are checked in alongside compressed logs. All runs executed
on x86-64 Linux with LeakSanitizer disabled under ptrace. Counts include invalid
input and do not establish complete safety or conformance.

## Retained-code differential campaign

The [four-boundary campaign](evidence/2026-09-08-8cb3e06/summary.json) at `8cb3e06`
completed 2,023,237 executions with AddressSanitizer and no findings:

| Target | Executions | Duration | Peak RSS |
| --- | ---: | ---: | ---: |
| Preprocessor | 1,111,545 | 901 s | 519 MiB |
| Semantics and layout | 492,670 | 901 s | 513 MiB |
| Bindings | 142,125 | 901 s | 513 MiB |
| Retained-code comparison | 276,897 | 901 s | 514 MiB |

The reports record fixed source and binary hashes, commands, limits, and sanitizer
settings. Starting corpus archives and compressed logs are checked in beside them;
each archive was verified against its recorded input hashes. This source includes
C11 atomic types, introspection, ARM declarations, native atomic headers, and plain
`__auto_type`. It precedes atomic Rust storage and explicit Clang/Linux profiles.
The target-aware harnesses used the five original profiles. All executions ran on
x86-64 Linux; LeakSanitizer remained disabled under ptrace. Counts include invalid
input and do not establish complete safety or conformance.

Two [earlier campaigns](evidence/checked-parser-campaigns-2026-09-08.json) completed
without findings: 195,925 executions at `caf6bcf` after GNU atomic intrinsics and
MMX, and 80,241 at `4945364` after parser limits, SSE, overflow intrinsics, and TLS.
Each ran for 901 seconds with AddressSanitizer, a 16 KiB input limit, a five-second
per-input timeout, and a 1 GiB RSS limit. Peak RSS was 709 MiB and 670 MiB,
respectively. LeakSanitizer remained disabled under ptrace. Neither run covers the
later C11 atomic types, type introspection, ARM, or `__auto_type` implementations.
These historical reports contain hashes; exact replay also requires the locally
saved starting corpus. New campaigns archive those inputs with their evidence.

The [definition follow-up](evidence/checked-definition-2026-09-08.json) found a
parameter-scope assertion after 179,902 executions: `int f(int named); int f() { return 0; }`
incorrectly inherited the declaration's parameter in the empty-list definition.
Commit `4962937` rejects that conflicting definition. The restarted campaign,
including the failing input, completed **243,023 executions in 901 seconds** with
AddressSanitizer and no findings. Peak RSS was 661 MiB. The report preserves both
runs, compiler validation, source hashes, and the fix; later intrinsic and parser
changes require separate validation.

The [retained-code campaign](evidence/checked-2026-09-08.json) completed **333,577
inputs in 901 seconds** without new findings at commit `ad162bd`. It used an immutable source snapshot
and copied AddressSanitizer binary, a 16 KiB mutation limit, a five-second timeout,
and a 1 GiB RSS limit. Peak RSS was 526 MiB. Each input selects one of the five
target profiles; all executions ran on native Linux.

An earlier run found that recording an invalid enum constant changed the diagnostic
for `enum{A=B};`. The analyzer now preserves the ordinary constant-evaluation error
before recording a successful expression, while preserving runtime-bound evaluation
contexts and cached scopes. The report retains the failed run, minimized reproducer,
fix commit, and replay results. The completed run started with 968 inputs drawn from
that saved corpus, repository seeds, and compiler-preprocessed c-testsuite cases.

A separate scan checked 224 preprocessed inputs on every target profile: 1,112
accepted results had identical declaration IR and valid graph links, and eight
rejections had identical diagnostics. The inputs include zlib, SQLite, zstd, and
libgit2 headers. This checks retention parity; preprocessed native typedefs are not
necessarily suitable for a different target.

The evidence records source and binary hashes, toolchains, input provenance,
commands, limits, and final libFuzzer statistics. Fuzzer counts include rejected C
and invalid UTF-8; they do not measure accepted programs or prove execution
semantics. LeakSanitizer remained disabled because process inspection is unavailable
under ptrace. A clean bounded run does not establish complete safety or conformance.

The expression-alignment seed covers packed fields, declared object alignment,
pointer-cast provenance, `_Generic`, and unevaluated VLA bounds. Exact replay
checks the public retained graph on all seven compiler profiles; see the
[alignment evidence](../corpus/evidence/expression-alignment-2026-09-08.json).
This replay is separate from the sanitizer campaigns above.

The [combined revision campaign](evidence/2026-09-08-bb6a401/summary.json) at
`bb6a401` completed 2,547,850 executions with ASan and no findings: 1,632,314
preprocessing, 453,296 semantic, 172,151 binding, and 290,089 checked-code inputs.
Each campaign ran for 900 seconds on x86-64 Linux; peak RSS was 513–519 MiB.
The archive includes original starting corpora, compressed logs, exact source and
binary hashes, commands, and toolchain versions. LeakSanitizer was disabled under
ptrace. These runs precede explicit C11/GNU11 modes and use the archived single-mode
harnesses. Execution counts include rejected inputs and do not establish conformance.

## Inline ownership replay

The [inline ownership evidence](evidence/inline-ownership-2026-09-08) preserves
three runs. The first exposed an outdated harness assertion equating every body
with the entity's latest body. The corrected invariant checks superseded bodies,
their original declaration sites, the later canonical body, and inline source
annotations. The core run passed 8,388 inputs; the final run including weak
composition passed 6,834 inputs in 121.566 seconds with 602 MiB peak RSS and no
artifacts. All 11 profiles and four language modes are seeded without changing
source bytes. Each run retains its initial corpus, dictionary, source manifest,
commands, and logs; the original failing input remains archived.

## C99 and C17 campaign

The runner verifies all 88 profile/mode settings for each of 137 seed files,
preserving every original source byte. Eight preprocessing seeds retain their
40 independent query, comment, trigraph, and scope-punctuator settings.

The [recorded campaign](evidence/c99-c17-2026-09-08/mutations/evidence.json.gz)
executes 22,764 inputs in 301 seconds, adds 792 corpus units, and reaches 624 MiB
peak RSS without artifacts. The initial 7,834-input replay is preserved separately;
its runtime was largely spent initializing the expanded seed set. Both runs retain
source hashes, starting corpora, dictionaries, commands, and toolchain identities.
