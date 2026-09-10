# Enum constant names

The Builder matches bindgen's `prepend_enum_name` setting, which defaults to
`true`. For `enum Mode { VALUE = 1 };`, integer output exposes `Mode_VALUE`.
Setting it to `false` exposes `VALUE`. A named enum uses its tag; an anonymous
enum uses the first typedef declared with its definition. Later aliases,
including `typeof` aliases, do not change the prefix. Enums with neither a tag
nor a direct typedef use their lexical record owner, if present.

Keyword components are escaped before joining: `enum type { match = 1 };`
produces `type__match_`. Name and file allowlists still select the original C
enumerator. The report retains that C name separately from the emitted Rust name.

Rust enum variants do not acquire the prefix. Named Rust enums expose values
through variants and associated aliases for repeated values, without additional
global integer projections. Truly anonymous Rust enums retain globals typed as
the generated Rust enum. Changing the prefix setting does not change their type
or discriminant representation.

## Lexical record names

The Builder qualifies a tag defined inside a record with its lexical owner.
For `struct Holder { enum Nested { VALUE = 1 } field; };`, the enum type is
`Holder_Nested`, its prefixed integer constant is `Holder_Nested_VALUE`, and
`rustified_enum("Holder_Nested")` selects it. The unqualified `Nested` pattern
does not select that Rust enum. C lookup and canonical tag identity stay the same:
an outside `enum Nested` reference still denotes the original C type.

An anonymous enum in `Holder` has a `Holder_VALUE` constant and an anonymous
helper type such as `Holder__bindgen_ty_1`. Its helper type or the original
enumerator name can select Rust enum output. Anonymous Rust enum constants keep
their owner prefix even when `prepend_enum_name(false)` is set. Anonymous records
and enums share one source-order counter per owner; a direct typedef supplies a
name without consuming that counter. Named and anonymous owners can nest.

A standalone file declaration made before a nested definition gives the tag a
file-level generated name. Ordinary pointer, prototype, and typedef uses do not
change an existing nested name. The semantic model retains the actual definition
owner separately from this prior-declaration fact. The sparse
`TranslationUnit::lexical_tags` maps also retain source order and the declaration
index of a direct anonymous typedef, without adding fields to each record or enum.

Generated type names are checked for collisions among emitted declarations. For
example, a separate `struct Holder_Nested` conflicts with the qualified enum.
Malformed public ownership graphs, excessive depth, and excessive name storage
produce generation errors.

## Macros and collisions

For an integer enum followed by `#define VALUE 2`, prefixing preserves both
`Mode_VALUE = 1` and the macro's `VALUE = 2`. Their shared original C spelling
does not erase the distinct generated enum constant. A selected macro or renamed
function that collides with an emitted constant produces a generation error.
This includes the duplicate `VALUE` definitions produced by bindgen when that
example disables prefixing. An unselected declaration does not cause a collision.

## Core library policy

`BindingOptions::prepend_enum_name` defaults to `false`, and
`EnumConstantStyle::Integer` preserves the core library and CLI's existing global
integer projections, including projections alongside Rust enum variants.
The Builder enables prefixing and `EnumConstantStyle::Bindgen` by default. A
library caller can select these options explicitly without changing C analysis.

## Discovery beneath enum cursors

For `enum Outer { COUNT = sizeof(enum Inner { VALUE = 1 }) };`, the Builder
omits `Inner` and `VALUE`, matching bindgen's cursor traversal. The semantic unit
still retains their C type identity, constants, scope, and layout. The core's
default integer output still exposes them.

A later declaration can expose the hidden type. For example, adding
`struct Owner { enum Inner field; };` exposes `Owner_Inner` and
`Owner_Inner_VALUE`; `rustified_enum("Owner_Inner")` selects that Rust enum.
Pointer, array, and function type wrappers instead discover their referenced
tags in the module naming context. A `sizeof` query on an existing type does
not expose it. Discovery follows reachable record contents, so references inside
another hidden record wait until that record is reached. The first visible use
also supplies file-allowlist provenance and anonymous helper ordering.

`TranslationUnit::tag_discovery` retains these sparse binding facts separately
from actual lexical owners. Units containing tag definitions under enum cursors
or new file-scope tags in trailing tag attributes receive one additional
bounded parse and graph walk after type checking. Other units retain no
occurrence graph and use no second parse. The graph
limits events and type traversal to one million entries or steps, and nesting
to 128 levels; invalid public IDs and naming owners produce generation errors.

General Rust type/variant keyword escaping and static constant object projection
remain separate policies. Enum tag alignment is limited to the
[supported storage-preserving forms](enum-alignment.md).

## Tags in trailing record attributes

For `struct Record { char field; }
__attribute__((aligned(sizeof(enum Visible { VALUE = 1 }))));`, the Builder
emits `Record_Visible` and `Record_Visible_VALUE`.
`rustified_enum("Record_Visible")` selects the Rust enum. The same enum in an
attribute before the record or between `struct` and its tag keeps the global
`Visible` name. A prior file-level enum declaration also keeps the global name.
An anonymous record's direct typedef supplies its prefix, and records nested in
the attribute retain their own children. These rules also apply when no tag is
hidden beneath an enum cursor elsewhere in the file.

The cursor owner is separate from C scope: `enum Visible` and `VALUE` remain
available at file scope, and their actual lexical ownership stays unchanged.
The core's default integer output does not use this naming override. Source
type checking, layout, and retained code share the same semantic result.

## Validation

The maintained [enum naming tests](../crates/toucan_bindings/tests/enum_names.rs)
and [lexical ownership tests](../crates/toucan_bindings/tests/lexical_enums.rs)
cover prefixes, selectors, anonymous numbering, collisions, and native C/Rust
layout and call checks. [Discovery tests](../crates/toucan_bindings/tests/tag_discovery.rs)
cover hidden tags and attribute placement; the
[adapter tests](../crates/toucan_bindgen/tests/tag_discovery.rs) check physical
file selection after discovery.

The [pinned naming capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/enum-constant-names-2026-09-08.json.gz),
[discovery capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/enum-cursor-discovery-2026-09-09.json.gz), and
[record-attribute capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/record-attribute-enum-names-2026-09-09.json.gz)
retain earlier bindgen comparisons and native observations. The last capture
records a deeply nested interior attribute rejected by that source revision;
it does not establish arbitrary attribute-nesting support. These comparisons
cover selected names, layouts, and calls, not complete textual or public API
equality or application performance.
