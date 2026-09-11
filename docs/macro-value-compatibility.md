# Bindgen macro values

The adapter's macro-value evaluator models bindgen 0.72.1's ordered
cursor-token evaluation. These values are not native C constant-expression
results. Builder generation combines written definition history, file selection,
callback classification, Rust type projection, and report provenance. Ordinary
`Compilation::bindings` continues to use C constant-expression evaluation.

## Values and history

Integers wrap as signed 64-bit values; integer suffixes do not supply C type
identity. Right shifts are arithmetic and shift counts are reduced modulo 64.
Arithmetic involving a float uses f64. Byte strings concatenate without target
wide-character conversion. Character values retain Unicode versus raw encoding
until a caller safely projects them; a character above the output range never
causes a panic here. Nonfinite floating values retain their bits.

Only the bounded flat literal parser in cexpr 0.6.0 is reused. The evaluator owns
operator parsing and checks integer division and remainder by zero. Logical
operators, comparisons, conditional expressions, casts, sizeof, macro expansion,
and function calls are outside cexpr's grammar. A character is not an arithmetic
operand in that grammar. Literal and keyword tokens follow the selected Clang
profile's language mode and target, independently of native C evaluation.

The context starts empty. Configured predefined macros are not implicitly added.
Successful definitions replace the context value. Only the first successfully
parsed definition can emit an item, and even Invalid reserves the name. An
unsuccessful parse preserves the earlier value. Undef does not clear this value
context. Source filtering must happen after the value-context update when the
reference has already parsed a macro.

With callbacks, Clang classifies a historical cursor using the name's final
active macro. Consequently every historical definition of a currently
function-like name is skipped, including earlier object-like definitions. Final
undef disables that classification. The caller passes this fact explicitly as
`skip_as_function_like`; the evaluator still uses each definition's written
parameter tokens when parsing is enabled. A known single parameter can therefore
parse as an expression: after `Known=9`, `#define A(Known) -3` evaluates to 6.

## API and bounds

`Context::new(Limits, CompilerProfile)` constructs an empty context.
`define(name, &Macro, skip_as_function_like)` returns an optional `Parsed`, which
contains an owned `Value` and `first_definition`. None represents caller-requested
function skipping. `note(name, Value)` records an already evaluated snapshot.
`Error` distinguishes parsing, unknown identifiers, keywords, invalid literals,
zero division, and resource limits.

Errors identify normalized replacement byte offsets. Synthetic parameter tokens
precede the replacement and use no offset; whole-definition/name errors and
end-of-input also use no offset. Physical source provenance remains with the
captured MacroDefinition.

Default bounds are 65,536 source bytes per definition, 4,096 tokens, 128 levels,
1 MiB per resulting byte string, 100,000 context entries, 64 MiB of retained
entry/value payload, and 256 MiB-equivalent work units. Work charges source
scanning, parser visits, literal parsing, aliases, string copies, and failed
attempts. Payload accounting does not claim exact allocator overhead. Lower
limits are supported; recursive descent always retains a hard 128-level ceiling.
Only successful value insertion changes context state.

## Builder representation and reports

`default_macro_constant_type(MacroTypeVariation::Unsigned)` is the default:
nonnegative values use u32/u64, and negative values use i32/i64. `Signed` selects
i32/i64 for all integers. `fit_macro_constants(true)` also considers 8- and
16-bit types. The last call to either option wins. Integer suffixes do not
override these settings. Characters use u8 when safely representable; f64 values
and byte strings retain their separate representations.

Builder records written definitions and uses the explicit `RecordAndReplace`
preprocessing policy. Unsupported expressions are reported as skipped macros.
Resource-limit errors fail generation even for macros outside selected files,
because those definitions participate in subsequent value lookup. The report's
`macro_evaluation` is `Provided`, and `macro_types` contains no invented C types.
Accepted redefinitions retain physical source locations on both binding APIs.

## Validation

The [macro-value tests](../crates/toucan_bindgen/tests/macro_values.rs) check
evaluator values, literal handling, and resource limits. The
[history tests](../crates/toucan_bindgen/tests/macro_history.rs) cover redefinition,
callbacks, and file selection. These are separate from complete consumer builds;
see [replacement readiness](replacement-readiness.md) for those gates.

The [historical evaluator capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/macro-values-2026-09-08/README.md)
and [Builder capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/builder-macro-values-2026-09-08/README.md)
retain the original reference comparisons and their configuration limits.
