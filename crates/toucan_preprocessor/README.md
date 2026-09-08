# toucan_preprocessor

A native C preprocessor for header consumers. The library does not invoke a compiler or
load libclang. Callers supply the target's include directories, predefined macros, and
optional fallback resource headers.

```rust
use std::path::Path;
use toucan_preprocessor::{Config, Preprocessor};

let mut preprocessor = Preprocessor::new(Config::default());
let result = preprocessor.preprocess_str(
    Path::new("example.h"),
    "#define COUNT 4\nstruct Example { int values[COUNT]; };\n",
)?;
assert_eq!(result.expand_object_macro("COUNT")?.as_deref(), Some("4"));
# Ok::<(), toucan_preprocessor::Error>(())
```

## Supported preprocessing

- Object and function macros, argument prescanning, recursive expansion suppression,
  stringification, token pasting, variadics, and GNU comma elision.
- Conditional groups and `defined`, with checked `intmax_t`/`uintmax_t` arithmetic and
  short-circuit evaluation. Ordinary character constants accept octal and hexadecimal
  escapes for every byte value. `Config::char_unsigned` selects the target's plain-char
  model independently of macro redefinitions; the integrated frontend sets it from
  the selected target. Preprocessing promotions use `uintmax_t` on unsigned-char
  targets, as in GCC and Clang. ASCII wide character constants use the explicit
  `__WCHAR_TYPE__` and `__WCHAR_UNSIGNED__` profile.
- Quoted and angle-bracket includes, `include_next`, `__has_include`, and `pragma once`.
  Explicit filesystem include paths take precedence over virtual resource headers.
- Trigraphs, escaped newlines, comments, digraphs, `__FILE__`, `__LINE__`, `__DATE__`, `__TIME__`, the GNU
  `__COUNTER__` extension, and `line` directives
  with C string escape decoding for filenames. GNU numeric line markers from compiler
  preprocessing output are accepted, including line zero and include transition flags.
- `_Pragma` operators, including macro-generated directives, share `pragma once`
  handling and `pragma pack` preservation with ordinary directives. Diagnostic and message pragmas
  are accepted without changing the generated source.

The `__clang__` predefined macro selects Clang's `include_next` behavior for quoted
local helper headers. Otherwise, these headers use GCC's search behavior. Standard
include directories and compiler feature-query macros are not inferred from the host.
The same profile selects Clang's support for `_Pragma` inside preprocessing conditions;
GCC rejects that use.

Each call starts a fresh translation unit. The result contains expanded source, final
macro definitions, and canonical paths of the filesystem dependencies actually read.
Object macros are expanded on request; an invalid unused macro need not invalidate the
header. Final-environment macro queries reject `__COUNTER__` and `_Pragma` because
these operations require the state and directive ordering of an active translation
unit. Counters reset at each preprocessing entry point.

Set `Config::allow_filesystem` to `false` to restrict an embedded or fuzzed preprocessor
to in-memory source and virtual headers. Filesystem entry points then return an error,
and include queries cannot observe local files.

## Compiler feature queries

`Config::feature_queries` accepts an immutable, caller-supplied capability catalog.
GNU query rules enable `__has_builtin`, `__has_attribute`, and `__has_c_attribute`.
Clang rules also enable `__has_feature`, `__has_extension`,
`__has_declspec_attribute`, and `__building_module`. The standalone default leaves
these operators undefined. The library does not infer capabilities from host tools
or compiler identity macros.

Feature, extension, and module queries read one unexpanded identifier. Attribute
queries expand their arguments; builtin queries expand arguments under GNU rules.
C-attribute queries accept `namespace::name`; GNU attribute queries do too.
Namespace separators are read before expanding that token. Ordinary wrapper-macro
argument prescanning still applies. Providers receive names and namespaces with
their original spelling and return numeric values; Clang results greater than one
carry the compiler's `L` suffix.

`Config::scope_punctuator` controls whether `::` is one preprocessing token.
Enable it for Clang and GNU language modes. Its default is `false`, matching strict
C tokenization: adjacent colons retain their lexical identity for GNU namespace
queries, while pasting two colons is invalid. Whitespace-separated colons and
colons joined by argument substitution do not become a namespace separator.
This setting is independent of query availability and predefined macro overrides.

Source `#define`/`#undef` and configuration overrides replace or remove operators.
Entry points restore the configured initial state; final-environment macro queries
observe the final operator state. Malformed active queries diagnose even on the
unevaluated side of `0 && query()`. Inactive directive groups do not evaluate them.
Expanded `_Pragma` directives inside query arguments remain unsupported.

## Translation timestamps

`Config::timestamp` fixes the UTC value of `__DATE__` and `__TIME__` for the entire
translation unit, including forced includes and final-environment macro queries.
**The library default is the Unix epoch**, yielding `"Jan  1 1970"` and
`"00:00:00"`. Reusing a preprocessor retains its configured timestamp. The library
does not read the clock, `SOURCE_DATE_EPOCH`, the locale, or the host time zone.

Supply a reproducible timestamp explicitly:

```rust
use toucan_preprocessor::{Config, PreprocessingTimestamp};

let config = Config {
    timestamp: PreprocessingTimestamp::from_unix_seconds(951_782_400)?,
    ..Config::default()
};
// __DATE__ is "Feb 29 2000"; __TIME__ is "00:00:00".
# Ok::<(), toucan_preprocessor::TimestampError>(())
```

Timestamps are whole Unix seconds in `0..=253402300799`, ending at
9999-12-31 23:59:59 UTC. Parsing accepts nonempty ASCII decimal digits, with no
sign, whitespace, or fraction. Date strings use English month abbreviations and
space-padded days, as specified for the [standard C macros](https://gcc.gnu.org/onlinedocs/cpp/Standard-Predefined-Macros.html).
Both macros are always defined. Source redefinitions, `#undef`, and replacements
through `Config::defines` are rejected, as for `__LINE__`; configure the timestamp
instead. Expansion locations identify the macro invocation.

The Toucan **CLI** supplies one captured wall-clock timestamp by default. When
[`SOURCE_DATE_EPOCH`](https://reproducible-builds.org/specs/source-date-epoch/) is set,
it supplies that value instead; invalid values fail before writing output. Both
CLI paths use UTC, independent of `TZ` and locale. The GNU `__TIMESTAMP__` extension
depends on file modification times and remains unsupported. It is not advertised
by `defined` or `#ifdef`; expanding it produces a diagnostic unless the caller
has supplied an ordinary macro replacement.

## Source locations

`Preprocessed::mappings` records ordered generated byte ranges and source anchors.
`resolve_location(offset)` resolves a preprocessed offset to a source line and one-based
byte column. Lines are one-based unless a GNU line marker explicitly selects zero. Filesystem includes retain canonical paths; virtual headers and
`#line` directives and GNU line markers retain their source names. Numeric markers
do not expand macros. Their include transitions are checked and bounded; filenames
do not trigger filesystem reads or change the physical directory used for includes.
System-header and implicit C-linkage flags are accepted without changing constraint
checking. These flags are not retained in source mappings.

Ordinary output tokens point to the start of the original token. Tokens produced by
macros point to the outer invocation and use `OriginKind::MacroInvocation`. Preserved
directives point to their directive start. Separator bytes and end of output use the
preceding anchor. These locations do not imply a macro definition location, a complete
expansion stack, or exact character correspondence within a spliced token. Mappings
remain valid while the returned source is unchanged.

## Limits

Source reads, cumulative included bytes, lexed tokens, replacement work, and generated
output have configurable budgets. Include and expansion depth are bounded; preprocessing
expressions additionally reject nesting beyond 128 parser frames. These are work and
input limits, not a process memory quota.

Unsupported features return diagnostics: non-ASCII identifiers, literal non-ASCII
or multicharacter preprocessing character constants, non-ASCII wide constants, `__VA_OPT__`, and unknown active
directives or pragmas. Header names retain literal backslashes; they are not decoded
as C strings. Filesystem lookups use host path conventions, including when target
macros describe another platform. Virtual header keys match the written name exactly.
Whitespace inside angle brackets is rejected. Empty names and NUL bytes are rejected.
`#line` filenames must decode to UTF-8 without NUL bytes.
This is not yet a complete C preprocessor conformance
implementation.

## Validation

`cargo test -p toucan_preprocessor` exercises macro rescanning, the recursive macro
example from the C standard, conditional arithmetic, resource limits, source locations,
and include search behavior. Differential tests invoke `CC` (default: `cc`) and compare
preprocessing tokens and filesystem include results. Run with both GCC and Clang when
changing expansion or include handling.
