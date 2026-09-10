# Fuzzing

The fuzz harnesses find panics, sanitizer failures, timeouts, and broken internal
invariants. Execution counts include rejected C and invalid UTF-8. They do not
measure semantic correctness or prove that generated bindings are usable.

The separate [native C/Rust audit](../corpus/conformance/generated-bindings.md)
compiles generated bindings, compares values and layouts with GCC and Clang, and
checks rejection of deliberately invalid C.

## Harnesses

| Target | Exercises |
| --- | --- |
| `parser` | Preprocessed C, AST spans, parser limits, and diagnostic formatting |
| `preprocess` | In-memory headers, macro expansion, source coordinates, comments, and resource limits |
| `semantic` | Declaration analysis and layout queries |
| `bindings` | Default binding generation and a second pass varying enum, trait, and function options |
| `checked` | Analysis with and without retained code, equal declarations/diagnostics, and retained graph invariants |

Inputs must be UTF-8. Parser, preprocessing, semantic, and checked inputs are capped
at 16 KiB; bindings at 8 KiB. Preprocessing cannot access the filesystem.

The three profile-aware harnesses share their selector implementation. The input
byte sum selects a compiler profile modulo `CompilerProfile::ALL.len()`; bits
8–10 select GNU11, C11, GNU90, C90, GNU99, C99, GNU17, or C17. No source prefix is
consumed. The runner queries the saved binary for its actual profile count before
padding each seed to cover every profile and language mode. Optional `--profiles`
is an assertion against that count; a stale value fails the campaign.

Parser seeds use trailing whitespace to cover all 64 language/flavor/extension
settings. Preprocessor seeds use trailing block comments to cover all 480
comment/query/trigraph/scope/history/redefinition/documentation settings. Binding
trait and function settings are additionally selected from input bytes; ordinary
seed padding does not guarantee every combination of those options.

## Run

Install `cargo-fuzz` and a nightly Rust toolchain. From the repository root:

```console
cargo +nightly fuzz run parser -- -max_total_time=60
python3 scripts/run_fuzz_campaign.py checked --seconds 900 --seed 12345 --output fuzz/runs/checked-local
```

Use `--toolchain ohm` to select Ohm for the runner's Cargo and rustc commands.
The output directory must be new. A campaign saves the starting corpus archive,
final corpus inventory, dictionary, executable, source hashes, commands, sanitizer
settings, logs, and reproducers. A changed source tree fails the run. Parser
campaigns track build inputs separately so documentation can change during a run.

CI runs a 60-second smoke test per harness on code changes. Scheduled and manual
jobs run each harness for 15 minutes with AddressSanitizer, preserve the evolving
corpus after successful default-branch runs, and upload artifacts for 30 days.
They also verify profile padding against the actual compiled selector.

To replay a campaign, extract its starting archive into a new directory and run
its saved executable with the recorded arguments, adjusting corpus, dictionary,
and artifact paths. Use a compatible host. A final corpus does not reproduce the
starting state, and source revisions may change selector mappings.

Minimize each finding and add a regression test. Short runs are smoke tests;
passing a bounded campaign does not establish complete safety. See
[historical captures](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/fuzz/evidence) for past results.
