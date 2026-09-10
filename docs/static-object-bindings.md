# Scalar object bindings

Builder emits supported scalar initializer values as Rust constants. The declared primitive or typedef type is preserved after C initializer conversion: `static signed char value=255;` emits a `c_schar` constant with value `-1`. Mutable, volatile, and thread-local objects use the same initializer projection. Rust constants represent an initializer snapshot; later writes to C storage do not change them.

The first written declaration in a selected physical header determines the binding. `extern const int value; const int value=7;` emits an extern object; reversing those declarations emits a constant. `allowlist_file` can select the later definition independently. Existing external-object name callbacks apply to constants, and internal objects retain their C names. External enum-typed objects remain extern declarations. Atomic scalar literals use their contained scalar representation.

Builder enables the optional [object occurrence catalog](object-values.md) and passes selected entries through `BindingOptions::object_bindings`. The core default leaves that map empty and preserves its existing static-object omission and extern-object policy. Supplied entries must match the target/compiler profile, declaration index, C name, and compatible type. The emitted scalar must agree with the destination type. Direct 128-bit integer objects preserve complete numeric values on supported Rust targets; see [128-bit object constants](int128-object-bindings.md) for the separate C ABI boundary. Floating types beyond `float`/`double` produce diagnostics. C values remain complete in the semantic catalog.

Unsigned values above `i64::MAX` follow bindgen's literal/unary fallback boundary: an ordinary integer literal can emit a constant, while a binary or cast initializer can retain an extern declaration for an external object. Internal objects without materialized constants are omitted and listed in `Report::skipped_declarations`. Name-selected objects still contribute their referenced types. This deliberately avoids bindgen's unresolvable extern projections for internal symbols; see [internal object bindings](internal-object-bindings.md).

## Related projections and validation

[String objects](string-object-bindings.md) and
[earlier const-object reads](const-object-initializers.md) have their own support
boundaries. Pointer-value copies and long-double bindings remain unsupported.

The maintained [object binding tests](../crates/toucan_bindgen/tests/object_values.rs) cover
initializer conversion, declaration selection, and callbacks. [Historical comparisons](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence)
retain the original bindgen controls and compiler runs. Consumer compatibility
requires testing the selected headers and generated bindings on the intended
target.
