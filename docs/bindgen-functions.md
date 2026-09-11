# Default Builder function selection

The bindgen adapter emits foreign declarations for ordinary externally linked C
functions, including functions whose bodies are present in the header. The caller
must link the C object that supplies each function. Internal-linkage functions
remain excluded.

Inline selection follows declaration-time facts. A non-inline declaration can
select a function unless an inline body exists. Thus a plain body followed by a
later inline prototype remains eligible, while an inline body excludes the
function even when GNU rules later replace that body. DLL storage and weak symbol
ownership do not change this source-selection rule. Existing unsupported Rust ABI
diagnostics, including optional weak-symbol linkage, still apply.

`Declaration::inline_facts` retains two facts from the existing bounded inline
history: whether a file-scope occurrence was non-inline when checked, and whether
any file-scope body was inline. An absent value means no inline history applies.
This is independent of `function_definition_kind`, which describes which body
supplies a linker definition. The optional field is additive in serialized units.
Caller-built inconsistent facts are rejected during binding generation.

The adapter enables `BindingOptions::emit_function_definitions` and
`exclude_inline_functions`. Both options default to false in the core emitter and
CLI. The inline filter checks the compact facts directly; it does not build a
string blocklist or enable origin/checked-code capture on ordinary Builder calls.
Callback/file selection reuses the same body fact while keeping per-occurrence
inline information for callback order.

## Validation

The [Builder selection tests](../crates/toucan_bindgen/tests/default_functions.rs)
compare the pinned bindgen decisions and callback sequences, while
[inline-fact tests](../crates/toucan_bindings/tests/inline_facts.rs) check the core
emission options. Native tests compile generated declarations and execute C/Rust
calls. The bindings fuzzer varies definition emission and inline exclusion
independently.

[Historical selection observations](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/bindgen-functions-2026-09-08.json.gz)
retain the original reference, runtime, and allocation measurements.
