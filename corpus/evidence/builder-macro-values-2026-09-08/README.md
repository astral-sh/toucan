# Builder macro values: 2026-09-08

All 73 paired cases match bindgen 0.72.1 with libclang 18.1.3 in generated
constant names, Rust types, and executable values. The same driver source uses
each generator's Builder API. The comparison includes all global constants,
including the enum constant in the history fixture.

- Four signed/fit settings cover 31 integer boundaries, suffixes, f64 values,
  byte strings, and characters.
- Forty-eight history cases and 16 final-function/callback cases exercise the
  complete Builder pipeline, including incompatible redefinitions.
- Four file-selection cases distinguish first parsed definitions, later context
  updates, failed-then-successful definitions, excluded dependencies, and logical
  `#line` names. A configured-macro control checks preprocessing without seeding
  command-line values into the parsed macro context.
- Both generators' output compiles with warnings denied and executes on current
  Rust. Scalar, file-selection, and configured cases also use actual Rust 1.64.
  All 164 executions pass. NaN values would compare by classification; other
  floats compare exact bits. No NaN case occurs in the scalar boundary fixture.
- The adapter suite and workspace all-target/all-feature Clippy pass. Focused
  tests cover omitted out-of-range character representations and fatal resource
  exhaustion in excluded files. Reference character tool failures remain in the
  earlier standalone evaluator capture; they are not rerun or counted as matches.

The capture is local Linux execution. It does not establish native macOS or
Windows execution, a generated AWS-LC consumer build, or current performance.
The original configured-alias assertion was corrected after independent bindgen
output showed that command-line aliases are omitted. The control now checks the
configured value through preprocessing and emits a written literal.

`manifest.json` records the parent, exact Rust source hashes, compiler and rlib
identities, and archive hash. `captures.tar.gz` contains the runnable comparison
script, driver, inputs, generated sources, consumer sources, reports, complete
commands/output, and validation logs. Adapt its machine-local paths when replaying.
Compiled binaries remain in the local cache and are identified by hash. Production
Rust source hashes were verified unchanged after the comparison.

The independent review found no reachable blocker. Its additional 32 paired
controls match names, types, and values across failed/Invalid definitions,
excluded context updates, failed later updates, final function classification,
keywords, configured names, and empty first definitions, with callback and file
selection combinations. Raw commands and source are preserved in
`independent-review.json.gz`; these controls are separate from the 73-case matrix.
