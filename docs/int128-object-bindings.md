# 128-bit object constants

A directly declared `__int128` or `unsigned __int128` object with a checked arithmetic initializer can emit a complete Rust `i128` or `u128` constant. These numeric values work on Rust 1.64. The value has no C storage or calling ABI, so the constant formatter can preserve all 128 bits without applying the older Rust ABI restriction.

Actual C storage, record fields, foreign parameters and return values, and C typedef emission retain the existing Rust 1.78 requirement. In particular, a `typedef __int128 Wide` declaration still reaches that type-emission requirement. The change introduces no new Builder methods, target support, or C ABI claims. The existing 64-bit unsigned fallback policy and nonstandard floating diagnostics are unchanged.

This deliberately differs from bindgen 0.72.1 where its Clang evaluation path narrows values to 64 bits. For `static const __int128 value=((__int128)1)<<100;`, the reference emits zero. Toucan emits `1267650600228229401496703205376`, matching C. Large unsigned 128-bit expressions also emit their checked values instead of falling back to extern declarations.

The saved Linux x86-64 comparison contains ten signed/unsigned controls: zero, small values, high bits, mixed limbs, signed minimum/maximum, negative values, and unsigned maximum/conversion from negative. GCC 13.3 and Clang 18 agree on all 20 C executions. All 20 Toucan-generated executions across Rust 1.98.1 and actual Rust 1.64.0 retain the complete type and bit pattern. The reference emits two correct constants, six truncated constants, and two extern declarations; its eight constants were also executed on both Rust versions and the differences are preserved in the evidence.

Seven focused object tests and Clippy pass. The tests retain old-Rust rejection for extern objects, record fields, foreign functions, and explicit typedefs using C 128-bit types. Sixty-one existing controls excluding the two changed 128-bit cases retain byte-identical output, status, and diagnostics against the frozen string-projection baseline. No additional platform matrix or downstream consumer claim is made here.
