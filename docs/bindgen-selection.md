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

Macro selection uses [ordered macro values](macro-value-compatibility.md).
Every definition updates the parsed-value context before file filtering; only
the first successfully parsed definition can supply output. Its accessed header
determines membership, independently of later redefinitions and logical `#line`
names. [Default function selection](bindgen-functions.md) extends declaration
selection without requiring origin capture. Derives and formatting are separate
adapter policies; [documentation attachment](documentation-emission.md) follows
the selected declarations.

## Validation

The maintained [root selection tests](../crates/toucan_bindings/tests/selection.rs)
check typed roots, dependencies, invalid IDs, and ABI guards.
[File-selection tests](../crates/toucan_bindgen/tests/file_selection.rs) cover
physical paths, redeclarations, callback precedence, and renamed blocklists.
[Origin tests](../crates/toucan/tests/declaration_origins.rs) check physical header
provenance independently of retained bodies.

The [historical comparison](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/bindgen-selection-2026-09-08.json.gz)
contains pinned bindgen callback/name comparisons, native C/Rust call fixtures,
and allocation observations for its source revision. Formatting, enum/macro
policies, and full consumer builds require their own validation.
