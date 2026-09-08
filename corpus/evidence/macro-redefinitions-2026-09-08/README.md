# Recorded macro redefinitions: 2026-09-08

This slice keeps strict preprocessing as the default and adds explicit
RecordAndReplace compatibility. Records retain the current source site; configured
definitions use None. The policy does not manufacture previous locations.

## Evidence

- Twelve inputs on both GCC13 and Clang18 match expanded output, `-dD` written
  history, and incompatible-redefinition warning counts (24 comparisons). Inputs
  cover equal/changed definitions, comments and whitespace, changed parameter
  names, object/function transitions, undef, inactive directives, repeated changes,
  and configured predefined definitions.
- Five ordinary regressions cover default errors, final expansion/history,
  physical sites across forced headers and line directives, explicit absent sites
  for configuration, and reset after success, syntax failure, and record-budget
  exhaustion. The native regression compares another nine inputs on both
  preprocessors. All preprocessor ordinary tests pass; workspace Clippy, fuzz
  Clippy, format checks, and the five campaign-runner tests pass.
- All 48 macro-history reference inputs now preprocess in compatibility mode and
  preserve the frozen evaluator's values. The two formerly rejected rows retain
  a VALUE redefinition at input.h:2:9. The evaluator is included only in the
  isolated proof driver; Builder integration remains separate.
- A 61.4-second AddressSanitizer campaign executes 230,095 inputs with no artifacts
  and a 462 MiB peak. Source hashes remain unchanged. Preprocessing selector
  version5 covers ten seeds in all160 comment/query/trigraph/scope/history/policy
  settings, plus invalid UTF-8:1,601 initial inputs. The final corpus holds3,075.
  These bounded checks supplement ordinary conformance testing.

## Reproduction

`captures.tar.gz` contains exact native inputs, commands, output/history streams,
reference and candidate macro values, test logs, driver sources and locks, and
ASan initial corpus/log/coverage. `manifest.json` records source and executable
identity. Machine-local paths identify captured inputs and cached binaries; adapt
those paths to another machine. The frozen ASan executable remains cached at
`macro-redefinitions-probes/asan/preprocess` with its recorded hash.

From the candidate source:

```sh
cargo test -p toucan_preprocessor
cargo test -p toucan_preprocessor --test macro_redefinitions -- --ignored
cargo clippy --workspace --all-targets --all-features -- -D warnings
python3 -m unittest discover -s scripts/tests -p test_fuzz_campaign.py
```

The ASan runner imports the repository's exact160-setting padding helper and
preserves original seed bytes. The initial count in summary.json is verified from
initial-corpus.tar.gz; summary-original.json retains the initial harness report,
which mistakenly labeled the final corpus count as initial. No execution,
artifact, timing, or source-integrity result was changed by that correction.

Retained-record limits account for entry/name/path payload independently of the
optional history budget. They are not a measurement of process or allocator
memory. The core strict path creates no record arena and skips the extra source
location lookup. No timing or allocation speedup is claimed.
