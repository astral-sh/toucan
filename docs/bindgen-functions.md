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

The [saved evidence](../corpus/evidence/bindgen-functions-2026-09-08.json.gz)
records 110 pinned bindgen 0.72.1/libclang 18.1.3 decisions on Linux x86-64 and
Windows x86-64, in GNU90/GNU11. Each is replayed with and without callbacks.
Weak ordinary definitions retain Toucan's explicit optional-symbol ABI error;
the remaining selected functions and callback sequences match the reference.
The regression fixture also runs across the supported targets; the originally
recorded evidence covers seven targets and predates Windows ARM64.

Generated ordinary-body, late-inline, and callback declarations pass 32 native
C/Rust executions: both generators, GCC 13/Clang 18, C O0/O2, current Rust/actual
Rust 1.64, and Rust O0/O3. Replaced GNU inline functions stay absent from both
outputs. The full workspace passes 839 tests (216 opt-in tests ignored).

`Declaration` remains 136 bytes, `Type` 40 bytes, and `TranslationUnit` 224 bytes
on the measured x86-64 host. Default Builder and core generation for 1, 100, and
1,000 ordinary prototypes have byte-identical output and identical allocation
counts/bytes against the frozen selection baseline. Nine real header routes keep
preprocessed text unchanged; their units differ only in the added inline facts,
and all eight requested Rust outputs remain byte-identical. A separate 100,
1,000, and 10,000 inline-function capture retains timing samples for the direct
filter; shared-host timing is evidence of scaling, not a release speed claim.

The bindings fuzzer varies definition emission and inline exclusion independently
using the low two selector bits. Archived campaigns retain their original harness
and selector interpretation.
