# Caller-supplied macro values

`Compilation::bindings_with_macros` combines checked C declarations with values
supplied by a separate evaluator. It does not expand or evaluate the frontend's
final macro environment. Calling the ordinary `bindings` method afterward still
uses that unchanged C environment.

The supplied map uses original C names and participates in the same root
selection, enum-name projection, collision checks, and Rust representation
validation as ordinary binding generation. The caller supplies omission
messages; a `None` value records a macro without an emitted value. Integer,
floating, and string counts include only selected values.

`MacroValue::RustInteger` specifies a Rust width, signedness, and bounded bit
pattern. Signed values use two's complement; unsupported widths and overflowing
patterns are errors. `RustFloat` retains a binary64 value's exact bits, including
signed zero and NaN payloads. These variants do not infer C types or apply the C
integer normalization policy. Existing C-typed value variants remain available
for callers that have independently evaluated their types.

Reports from this API include `macro_evaluation: "provided"`. The ordinary C
path keeps its existing serialized report shape. `macro_types` continues to
record only transformations of explicitly C-typed integer values; it does not
invent C widths or signedness for Rust representations. Checked declaration
layouts and calling conventions remain properties of the compilation's target.

Float constants use const `from_bits` from Rust 1.83 onward. Older targets retain
the equal-width transmute needed for const evaluation. A scoped lint allowance
keeps that required compatibility expression warning-free on current compilers
and allows the unknown lint name on older compilers.

## Validation

Focused tests check report provenance, unchanged C state, typed root selection,
coexisting enum and macro values, and rejection of invalid integer storage.
Generated integer boundaries, negative zero, and a NaN payload compile and run
with warnings denied on current Rust and actual Rust 1.64. This is a supplied
value API; the bindgen-compatible evaluator and unchanged consumer comparisons
are separate layers.
