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

## Evidence and remaining boundaries

The [pinned bindgen 0.72.1 capture](../corpus/evidence/enum-constant-names-2026-09-08.json.gz)
covers default/true/false settings, tags,
typedef aliases, repeated values, keyword names, macro shadowing, and collisions.
Generated constants and enum values compile and cross C calls with GCC and Clang,
using current Rust and actual Rust 1.64 consumers.

The [lexical ownership capture](../corpus/evidence/nested-enum-names-2026-09-08.json.gz)
closes the seven nested-record differences in the earlier capture. It also covers
forward declarations, anonymous owner numbering, direct and later typedefs,
selectors, and native C/Rust layout and call checks.

On x86-64 Linux, `Record` remains 88 bytes and `Enum` remains 56 bytes.
`TranslationUnit` grows by 48 bytes for the two empty maps. Ninety-seven ordinary
profile/scaling controls retain their allocation counts; semantic and facade
controls request 48 additional bytes in the existing unit allocation. Across
the GCC and Clang zlib, SQLite, zstd, and libgit2 header routes, sparse origins add
1–23 allocation calls and 680–14,776 requested bytes. The unchanged AWS-LC wrapper
adds 7 calls and 4,568 requested bytes. All eight core-default binding files stay
byte-identical, with declaration origins enabled and disabled. These are
allocation measurements, not a timing comparison or an AWS-LC consumer result.

The [nested-name stack integration](../corpus/evidence/nested-enum-names-root-integration-2026-09-09.json.gz)
passes 920 workspace tests, workspace and fuzz Clippy, and eight native C/Rust
settings each on current Rust and actual Rust 1.64. The binding fuzz harness now
varies lexical enum naming and name prefixes while retaining its default pass;
this addition is not a new sanitizer-run claim.

One separate cursor-visibility difference remains: for
`enum Outer { COUNT = sizeof(enum Inner { VALUE = 1 }) };`, bindgen does not
emit the inner tag or enumerator, while Toucan retains them as valid file-scope C
declarations. General Rust type/variant keyword escaping and macro-history output
remain separate policies. These comparisons do not claim full textual or public
API equality.
