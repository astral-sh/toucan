# Bindgen macro values

The adapter's standalone macro-value evaluator models bindgen 0.72.1's ordered
cursor-token evaluation. These values are not native C constant-expression
results. This prerequisite is tested independently; Builder integration supplies
selection, callbacks, Rust type projection, and report provenance separately.

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

## Evidence and remaining integration

The [capture](../corpus/evidence/macro-values-2026-09-08/README.md) compares 48
history cases, 16 final-function cases, eight literal-token modes, 390 keyword
spellings across 24 mode/target settings, and 2,516 bounded cexpr expressions.
Additional probes distinguish successful reference values from four reference
aborts on out-of-range characters. Nine focused tests pass normally and under
AddressSanitizer, including 2,048 bounded malformed inputs and growth limits.

Toucan's strict preprocessor still rejects incompatible macro redefinitions that
Clang admits with a warning. The two reference rows exercising that difference
use independently captured definitions for this evaluator-only comparison; they
do not establish full-pipeline acceptance. An explicit compatibility policy with
retained diagnostics is separate work. Enum constants are excluded from the
macro-only comparison. Final Builder output and unchanged-source AWS-LC builds
remain integration gates.
