# Internal object bindings

An internal C object has no external linker symbol. Toucan emits a selected
internal object when it can materialize its scalar or string value as a Rust
constant. Otherwise, the primary object path omits it and includes its name in
`Report::skipped_declarations`. Name-selected internal objects still contribute
their referenced types through the ordinary bounded dependency collector.

This preserves integer, floating, Boolean, string, braced scalar, atomic and
supported 128-bit constants. Unsigned 64-bit literal and unary initializers above
`i64::MAX` retain their full values. Binary and cast initializers at that width
can have checked values without satisfying the reference constant-projection
rule; internal objects in those cases are omitted and reported. Internal enum,
aggregate, uninitialized and nonconstant pointer objects are also omitted when
they have no materialized constant. External objects retain their existing
extern fallback.

The explicit `BindingOptions::additional_objects` API diagnoses an internal
occurrence without a materialized constant. An explicitly requested extra Rust
name cannot become an extern declaration for an unavailable C symbol.

## Evidence

The [saved Linux x86-64 controls](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/internal-object-bindings-2026-09-09.json.gz)
preserve the original and corrected outputs for eleven settings against bindgen
0.72.1/libclang 18.1.3. This is an intentional compatibility difference: native
bindgen emits extern declarations for the unsupported internal cases. Twenty
Rust links against GCC/Clang objects confirm that five such projections in the
reference and preceding Toucan output cannot resolve their C symbols.

Twelve native-linked executions confirm that all five supported constants retain
their values across the reference, preceding Toucan and corrected Toucan output,
GCC/Clang, and current Rust/Rust 1.64. The C implementation also supplies a public
function that verifies the scalar values. Original evidence remains unchanged;
the saved replay records every command, output and candidate binary hash.
