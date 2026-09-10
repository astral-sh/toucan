# File object initializer values

`AnalysisOptions::retain_object_values` records file object declarations in source order, including declarations without initializers. Each entry identifies its canonical declaration, original name offset, type at that occurrence, linkage, thread storage, and supported destination-converted arithmetic value. Capture remains independent of `retain_code` and declaration origins. Ordinary analysis and binding generation keep their existing defaults.

The catalog preserves redeclarations because binding generators can distinguish `extern const int x; const int x=7;` from the reverse order. The former emits an extern object in bindgen 0.72.1; the latter emits a constant. Values are captured after existing C initializer validation and use the same integer and floating conversions. A missing value does not mean zero. Enum objects, pointers, records, arrays, and vector objects currently have no scalar value entry; atomic scalar values describe the contained scalar, not atomic storage.

Capture is limited to 100,000 occurrences, 64 MiB of accounted metadata, and 128 levels of owned type or scalar initializer nesting. Type and name storage are charged before copying. Parsed expressions and function bodies are dropped normally. No object value is added to the C integer-constant-expression environment.

See [scalar object bindings](static-object-bindings.md) for the binding policy
and [128-bit constants](int128-object-bindings.md) for values that exceed the
reference generator's integer range.

## String literal facts

`ObjectOccurrence::string_literal` retains direct ordinary/UTF-8 initializer bytes through the literal's final implicit NUL, including embedded NULs and subsequent bytes. Its C array type retains the declared bound independently. Parenthesized or computed expressions and wide encodings have no byte-literal entry. See [string object bindings](string-object-bindings.md) for the separate logical C-string projection, bounded native checks, and unterminated-array diagnostic.

## Earlier const definitions

The Clang analyzer can use [completed file const scalars](const-object-initializers.md) in later static initializers and constant-query builtins. This transient value table is separate from the optional written-occurrence catalog and from C integer-constant-expression lookup. Captured occurrences preserve their original order and destination values.
