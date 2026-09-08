# Enum constant names

The Builder matches bindgen's `prepend_enum_name` setting, which defaults to
`true`. For `enum Mode { VALUE = 1 };`, integer output exposes `Mode_VALUE`.
Setting it to `false` exposes `VALUE`. A named enum uses its tag; an anonymous
enum uses its first typedef name. Later aliases do not change the prefix.
Enums with neither a tag nor a typedef keep their enumerator names.

Keyword components are escaped before joining: `enum type { match = 1 };`
produces `type__match_`. Name and file allowlists still select the original C
enumerator. The report retains that C name separately from the emitted Rust name.

Rust enum variants do not acquire the prefix. Named Rust enums expose values
through variants and associated aliases for repeated values, without additional
global integer projections. Truly anonymous Rust enums retain globals typed as
the generated Rust enum. Changing the prefix setting does not change their type
or discriminant representation.

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

## Evidence and remaining boundaries

The [pinned bindgen 0.72.1 capture](../corpus/evidence/enum-constant-names-2026-09-08.json.gz)
covers default/true/false settings, tags,
typedef aliases, repeated values, keyword names, macro shadowing, and collisions.
Generated constants and enum values compile and cross C calls with GCC and Clang,
using current Rust and actual Rust 1.64 consumers.

The semantic model does not yet retain an enum's enclosing record for generated
names. Bindgen's `Holder_Nested_VALUE` and `Holder_VALUE` spellings therefore
remain outside this C-name policy, as do its generated nested-name selectors.
The capture preserves those differences. General Rust type/variant keyword
escaping and macro-history output remain separate policies; this layer does not
claim full textual or public API equality.
