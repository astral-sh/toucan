# String object bindings

Builder projects direct ordinary and UTF-8 string initializers as references to byte arrays. For example, `const char text[]="abc";` emits a four-byte Rust constant containing `abc` and its terminating NUL. Signed and unsigned character arrays retain the same byte patterns. Narrow character typedefs and pointer typedefs are supported.

A projection contains the logical C-string prefix through its first NUL. This can be shorter than the C object's storage: `char text[8]="abc"` has eight initialized bytes, and `char text[]="a\0bc"` contains bytes after its first NUL. The semantic occurrence retains the full literal bytes, including those bytes and the literal's implicit terminator, plus the declared C array bound. Projection leaves those facts unchanged. Array padding is not materialized in the captured literal.

An array without a NUL within its declared bound produces a precise projection diagnostic. `char text[3]="abc"` contains exactly three C bytes; bindgen 0.72.1 instead emits four Rust bytes with an added NUL. Toucan does not claim those are equivalent. Initializers too long even after dropping the final terminator remain subject to the frontend's existing rejection; permissive GCC/Clang truncation is separate conformance work.

Parentheses, braces, casts, pointer arithmetic, conditional expressions, `__builtin_choose_expr`, `_Generic`, and GNU `__extension__` prevent literal projection, matching the reference's written-expression rule. Wide, UTF-16, and UTF-32 initializers keep their typed C storage bindings. Unsupported or mixed literal encodings retain their existing frontend diagnostics. Atomic pointers also keep storage bindings. The first selected declaration, physical file selection, and external name callbacks follow the [scalar object rules](static-object-bindings.md).

Capture is opt-in and independent of body retention. It charges the literal's source-byte upper bound before decoding and allocating retained byte storage, within the existing 64 MiB object-metadata budget. Ordinary core emission keeps its existing defaults.

## Evidence

The saved comparison covers 50 headers against bindgen 0.72.1 and Clang 18. Forty-five accepted object kinds match; both frontends reject the invalid mixed-encoding header. One unterminated array receives the explicit projection diagnostic, and three truncated initializers reach the existing frontend admission boundary. This closes eight of the nine earlier string-projection differences; the ninth is the documented unterminated-array case.

Twenty-three direct string controls produce matching Rust array types and logical C-string prefixes: 92 executions across both generators and Rust 1.98.1/actual Rust 1.64.0, plus 46 GCC/Clang executions. Native probes record complete bounded C storage and its terminated prefix separately. Eight additional C executions record the four array-boundary cases, including the absence of a terminator. Every read stays within the declared array or the known backing literal. All 45 accepted candidate modules and 49 accepted reference modules compile under both Rust toolchains with warnings denied: 188 metadata compilations.

Eight physical file/callback controls retain the scalar layer's declaration ordering. Focused capture/projection tests, the metadata-budget test, and Clippy pass. The comparison concerns string projections; it does not claim complete API equality for surrounding atomic or typedef representations, real-project performance, or downstream consumer readiness.

The [combined-source checks](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/string-object-root-integration-2026-09-09.json.gz) repeat the 50 reference cases, 23 logical byte-prefix comparisons, and four array boundaries. All 92 Rust executions, 46 paired C executions, eight bounded C boundary checks, and 188 Rust metadata compilations pass. Eight file/callback selections match names and object kinds. Focused tests and workspace Clippy pass.
