# String object bindings

Builder projects direct ordinary and UTF-8 string initializers as references to byte arrays. For example, `const char text[]="abc";` emits a four-byte Rust constant containing `abc` and its terminating NUL. Signed and unsigned character arrays retain the same byte patterns. Narrow character typedefs and pointer typedefs are supported.

A projection contains the logical C-string prefix through its first NUL. This can be shorter than the C object's storage: `char text[8]="abc"` has eight initialized bytes, and `char text[]="a\0bc"` contains bytes after its first NUL. The semantic occurrence retains the full literal bytes, including those bytes and the literal's implicit terminator, plus the declared C array bound. Projection leaves those facts unchanged. Array padding is not materialized in the captured literal.

An array without a NUL within its declared bound produces a precise projection diagnostic. `char text[3]="abc"` contains exactly three C bytes; bindgen 0.72.1 instead emits four Rust bytes with an added NUL. Toucan does not claim those are equivalent. Initializers too long even after dropping the final terminator remain subject to the frontend's existing rejection; permissive GCC/Clang truncation is separate conformance work.

Parentheses, braces, casts, pointer arithmetic, conditional expressions, `__builtin_choose_expr`, `_Generic`, and GNU `__extension__` prevent literal projection, matching the reference's written-expression rule. Wide, UTF-16, and UTF-32 initializers keep their typed C storage bindings. Unsupported or mixed literal encodings retain their existing frontend diagnostics. Atomic pointers also keep storage bindings. The first selected declaration, physical file selection, and external name callbacks follow the [scalar object rules](static-object-bindings.md).

Capture is opt-in and independent of body retention. It charges the literal's source-byte upper bound before decoding and allocating retained byte storage, within the existing 64 MiB object-metadata budget. Ordinary core emission keeps its existing defaults.

## Validation

The maintained [semantic capture tests](../crates/toucan_semantic/tests/object_values.rs)
check literal bytes, declared bounds, and written-expression restrictions.
[Adapter projection tests](../crates/toucan_bindgen/tests/object_values.rs) check
terminated prefixes, unterminated-array diagnostics, declaration order, and
file/callback selection.

The [historical comparison](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/string-object-root-integration-2026-09-09.json.gz)
contains pinned bindgen controls and native C/Rust value checks. Its C probes
record full bounded storage separately from the logical C-string prefix and
never read beyond the declared array or backing literal. Those results apply to
the recorded revision; they do not establish complete surrounding API equality,
performance, or downstream consumer readiness.
