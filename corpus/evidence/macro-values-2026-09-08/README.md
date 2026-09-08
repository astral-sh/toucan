# Ordered macro values: 2026-09-08

This is a standalone evaluator comparison against bindgen 0.72.1/Clang 18.1.3,
not a claim of complete Builder or native C constant-expression equality.

## Results

- All 48 historical macro-value rows match. The enum constant in the enum-alias
  fixture is outside this comparison. Two incompatible-redefinition rows require
  separately captured definitions because the strict frontend rejects their
  full source; that error is retained in the result.
- Sixteen additional rows cover known function parameters, keywords, strings,
  wrapping shifts, overflow, and nonfinite floats. Twelve compare successfully;
  four reference aborts on out-of-range characters remain tool failures. The
  candidate preserves those character values for safe later projection.
- Sixteen final-function rows establish that callback skipping uses the final
  active definition, including the effects of undef and later object macros.
- Eight literal-token modes establish C11 prefix recognition and the absence of
  u8 character-token recognition in every supported mode.
- 390 candidate keyword spellings compare across 24 settings (Linux, Darwin,
  Windows, each in eight language modes). The source roster URL and hash are in
  keyword-reference.json; `defined` is excluded because Clang rejects it as a
  macro name. This is compilation on the local Linux machine using explicit
  cross-target profiles, not native Mac/Windows execution.
- Nine focused tests pass normally and under AddressSanitizer. They include
  2,516 fixed small expressions compared to cexpr, 2,048 deterministic malformed
  inputs, context/alias/string limits, zero division, and a hard recursion ceiling.
  The unbounded cexpr expression API is used only in the fixed differential test;
  production uses its flat literal parser. Package Clippy passes. Source hashes
  remain unchanged during the captured validation sequence.

## Reproduction

`captures.tar.gz` contains exact inputs, reference/candidate results, commands,
runners, driver source and lock, and validation logs. `manifest.json` records
source and executable identity. The source is 55aff58 plus this standalone
module, its tests, and dependencies. No Builder path calls it in this layer.
The durable instrumented test executable is cached locally at
`macro-compat-evaluator-probes/asan-tests` with its recorded hash.

From the candidate source:

```sh
cargo test -p toucan_bindgen --test macro_values
cargo clippy -p toucan_bindgen --all-targets --all-features -- -D warnings
```

The captured `validate.py` records the nightly AddressSanitizer invocation.
`compare.py` runs the source-specific driver and compares macro values, preserving
preprocessor failures and reference tool aborts separately. It normalizes only
NaN sign/payload when comparing emitted f64::NAN; raw float bits remain captured.
Reference probe runners use the recorded pinned helper binary and libclang path.
The keyword runner reads Clang18.1.3 TokenKinds.def from the URL/hash recorded in
keyword-reference.json. Adapt machine-local paths when replaying elsewhere.
