# Binding roots, header selection, and generated names

`BindingOptions::selection` accepts typed roots from the translation unit used
for generation. Declaration, record, and enum IDs are validated before use;
typedef and constant roots use exact names. `Some(BindingSelection::default())`
selects nothing. Name allowlists and explicit roots select their union. Required
type dependencies and existing ABI checks still apply.

`BindingOptions::generated_names` changes the Rust names of selected foreign
functions and objects while retaining their original C or explicit assembler
link names. Invalid names and collisions between selected Rust values fail
generation. `emit_function_definitions` permits foreign declarations for ordinary
externally linked function bodies; the core option defaults to false.

The bindgen adapter's `allowlist_file` anchors regular expressions to the whole
compiler-visible accessed header name, independently of diagnostic `#line`
names. It uses an optional lightweight declaration/file catalog. It does not
retain expression, statement, or initializer graphs. See the
[callback API](../crates/toucan_bindgen/src/callbacks.rs) for callback order and
[multiple object names](multiple-object-names.md) for redeclaration selection.

## Evidence

The [saved comparison](../corpus/evidence/bindgen-selection-2026-09-08.json.gz)
pins bindgen 0.72.1 and libclang 18.1.3. Its 16 paired cases compare callback logs
and selected source functions, objects, records, and aliases. They cover relative
main/include paths, file order, redeclarations, callback precedence, post-rename
blocklists, and a selected function whose record and callback dependencies live
in an excluded header. Toucan's implicit `__toucan_va_list_tag` support record is
recorded separately. Formatting and enum/macro policies are separate comparisons.

Two generated C/Rust fixtures pass all 64 execution combinations: reference or
Toucan output, GCC 13 or Clang 18, C O0/O2, current Rust or actual Rust 1.64, and
Rust O0/O3. They exercise renamed functions and objects, an assembler link name,
callbacks, and by-value record parameters/results. This is focused ABI evidence;
it does not establish a completed AWS-LC replacement build.

The frozen selection workspace has 836 passing tests (216 opt-in tests remain ignored by that
run). Workspace and fuzz Clippy plus Rustdoc pass. Nine unchanged header routes
have identical preprocessed text and ordinary semantic units, and their eight
requested Rust outputs remain byte-identical. Default core generation on 1,
100, and 1,000 declarations has identical allocation counts and bytes. Default
Builder generation removes 15 allocations by replacing its synthetic wrapper
with ordered header preprocessing; its generated output remains byte-identical.

The [root integration](../corpus/evidence/bindgen-selection-root-integration-2026-09-08.json.gz)
preserves the earlier enumerator-query optimization and macro-definition history.
The binding and origin suites and workspace Clippy pass on that combined source.

Macro selection now uses [ordered macro values](macro-value-compatibility.md).
Every definition updates the parsed-value context before file filtering; only
the first successfully parsed definition can supply output. Its accessed header
determines membership, independently of later redefinitions and logical `#line`
names. [Default function selection](bindgen-functions.md) extends declaration
selection without requiring origin capture. Derives and formatting are separate
adapter policies; documentation emission remains an integration gate.
