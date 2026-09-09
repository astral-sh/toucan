# Scalar object bindings

Builder emits supported scalar initializer values as Rust constants. The declared primitive or typedef type is preserved after C initializer conversion: `static signed char value=255;` emits a `c_schar` constant with value `-1`. Mutable, volatile, and thread-local objects use the same initializer projection. Rust constants represent an initializer snapshot; later writes to C storage do not change them.

The first written declaration in a selected physical header determines the binding. `extern const int value; const int value=7;` emits an extern object; reversing those declarations emits a constant. `allowlist_file` can select the later definition independently. Existing external-object name callbacks apply to constants, and internal objects retain their C names. Enum-typed objects remain extern declarations. Atomic scalar literals use their contained scalar representation.

Builder enables the optional [object occurrence catalog](object-values.md) and passes selected entries through `BindingOptions::object_bindings`. The core default leaves that map empty and preserves its existing static-object omission and extern-object policy. Supplied entries must match the target/compiler profile, declaration index, C name, and compatible type. The emitted scalar must agree with the destination type. Direct 128-bit integer objects preserve complete numeric values on supported Rust targets; see [128-bit object constants](int128-object-bindings.md) for the separate C ABI boundary. Floating types beyond `float`/`double` produce diagnostics. C values remain complete in the semantic catalog.

Unsigned values above `i64::MAX` follow bindgen's literal/unary fallback boundary: an ordinary integer literal can emit a constant, while a binary or cast initializer can retain an extern declaration. Objects without projected values, including internal objects, become extern declarations in Builder mode. An internal C symbol may need a C wrapper before a Rust use can link.

## Evidence

The frozen comparison contains 63 bindgen 0.72.1/Clang 18 GNU11 controls. The adapter matches the object kind for 44 accepted controls and rejects the same three invalid initializers. Nine string projections and four Clang const-object-read extensions remain separate work. That frozen comparison predates the [128-bit constant follow-up](int128-object-bindings.md), which replaces its integer projection diagnostics with complete C-validated values. Long-double bindings remain unsupported.

All 26 paired scalar controls have equal Rust types and values under current Rust 1.98.1 and actual Rust 1.64.0, with both GCC 13.3 and Clang 18 C values: 104 Rust executions and 52 C executions. All 53 accepted candidate modules compile under both Rust toolchains with warnings denied (106 metadata compilations). Eight file-filter/callback combinations match their reference declarations. The final candidate reproduces every earlier candidate output and diagnostic across all 63 controls after profile/type validation was added.

Focused tests also preserve ordinary analysis for ten existing initializer forms, including address-to-Boolean, constant queries, generic/choose expressions, enum-in-sizeof, omitted conditionals, infinity/NaN, pointer offsets, and aggregate initialization. No additional body graph is retained. This layer contains no real-project performance or consumer claim; those need validation after the remaining adapter layers are composed.

Subsequent [string-object support](string-object-bindings.md) and [earlier const-object reads](const-object-initializers.md) close their respective parts of the historical comparison. The latter closes three scalar-read cases; pointer-value copies remain separate work.

The [combined-source validation](../corpus/evidence/static-object-root-integration-2026-09-09.json.gz) repeats all 63 reference controls and the 26 scalar value cases: 104 Rust executions and 52 GCC/Clang executions agree. Focused object tests, binding-library tests, and workspace Clippy pass. The enum-discovery comparison improves to 98 of 101 public-name sets; the two object-constant differences are resolved, while record-attribute naming and aligned-enum support remain separate.

The [combined 128-bit checks](../corpus/evidence/int128-object-root-integration-2026-09-09.json.gz) repeat 20 GCC/Clang executions and 20 matching Rust 1.64/current executions. The archive preserves the reference’s truncated values separately. Focused object tests and workspace Clippy pass.
