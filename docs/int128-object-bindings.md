# 128-bit object constants

A directly declared `__int128` or `unsigned __int128` object with a checked arithmetic initializer can emit a complete Rust `i128` or `u128` constant. These numeric values work on Rust 1.64. The value has no C storage or calling ABI, so the constant formatter can preserve all 128 bits without applying the older Rust ABI restriction.

Actual C storage, record fields, foreign parameters and return values, and C typedef emission retain the existing Rust 1.78 requirement. In particular, a `typedef __int128 Wide` declaration still reaches that type-emission requirement. The change introduces no new Builder methods, target support, or C ABI claims. The existing 64-bit unsigned fallback policy and nonstandard floating diagnostics are unchanged.

This deliberately differs from bindgen 0.72.1 where its Clang evaluation path narrows values to 64 bits. For `static const __int128 value=((__int128)1)<<100;`, the reference emits zero. Toucan emits `1267650600228229401496703205376`, matching C. Large unsigned 128-bit expressions also emit their checked values instead of falling back to extern declarations.

The [object-value tests](../crates/toucan_bindgen/tests/object_values.rs) cover
signed and unsigned boundaries, generated values, and the separate Rust-version
restrictions on C storage and signatures. Earlier compiler comparisons remain in
the [historical archive](validation.md#historical-results).
