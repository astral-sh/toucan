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
- `push_macro` and `pop_macro` pragmas save and restore definitions on per-name
  stacks, including names that were undefined when pushed. They work across
  included headers and through `_Pragma`; restoration takes effect before the
  following tokens expand. An unmatched pop produces a diagnostic. Retained
  snapshots share the configured source-byte limit.

The `__clang__` predefined macro selects Clang's `include_next` behavior for quoted
local helper headers. Otherwise, these headers use GCC's search behavior. Standard
include directories and compiler feature-query macros are not inferred from the host.
A configured query dialect selects support for `_Pragma` inside preprocessing
conditions independently of identity macro overrides: Clang permits it and GNU
rejects it. Without a query configuration, `__clang__` selects the existing standalone
behavior.

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
Expanding queries preserve supported `_Pragma` effects, including `once` header
inclusion. Clang also accepts diagnostic and message pragmas there; GNU rejects its
`GCC diagnostic` and `message` handlers at this boundary. Raw queries and raw
namespace lookahead do not consume deferred pragmas from wrapper arguments.
`pack` inside query arguments remains explicitly unsupported: preprocessing-only
acceptance does not establish support during C compilation.

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
byte column. Lines are one-based unless a GNU line marker explicitly selects zero.
Filesystem diagnostics retain compiler-visible access names; canonical filesystem
identities remain available in dependencies and optional file origins. Virtual headers and
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

### File access names and ordered inputs

Filesystem dependencies and `#pragma once` use canonical file identities. Quoted
includes and `__has_include` resolve from the compiler-visible access directory,
including a symlink's directory. Literal `./` and `../` components remain in input
names. GNU uses each access spelling; Clang reuses the first registered name of a
physical file for both include lookup and `__FILE__`. Diagnostic `#line` names
change `__FILE__` and diagnostics without changing physical include resolution.

`Preprocessor::preprocess_files(&[PathBuf])` accepts an ordered, nonempty header
list in one macro environment. The final path is the main file and must exist as
written; earlier paths use compiler `-include` lookup (working directory, then
configured include directories). Configured in-memory forced includes run first.
Clang registers the main name before these forced headers. Main input is processed
even if an earlier forced header marked the same file `once`; Clang ignores `once`
in the main file, whereas GNU applies it to later includes. The single-file entry
point uses the same rules without constructing a header list. The facade exposes
this through `toucan::parse_files`.

Canonical paths currently coalesce symlinks, but distinct hard links remain
separate identities. See [the native path evidence](../../docs/header-paths.md)
for tested behavior and the remaining host file-identity work.

### Optional physical header origins

`Config::record_file_origins` records physical input paths in
`Preprocessed::file_origins()` independently of diagnostic `#line` mappings.
`source_file(offset)` resolves a token to its canonical filesystem identity
(or supplied in-memory name);
`source_name(offset)` returns its compiler-visible access spelling. Each
`FileMapping` exposes the same pair through `path()` and `accessed_path()`.
Adjacent ranges are coalesced only when both identity and exact spelling match.
Macro-generated declarations belong to the invocation's header.
`macro_definition(name)` gives the physical path and line of the final active
definition, while `macro_definition_name(name)` gives its access spelling.
`#undef` removes both facts; command-line definitions have no source origin.
A fresh run resets the catalog, and the default configuration allocates none of
it. Existing source, include, token, and expansion limits still apply. Clang's
first-name cache shares the ordinary dependency index and retains an additional
path only when the first name differs from its canonical identity. `push_macro`
and `pop_macro` restore the saved definition's origin and access spelling; a
restored undefined name has no definition origin.

### Optional macro definition history

`Config::record_macro_definitions` captures successful active `#define` directives
in `Preprocessed::macro_definitions()`. Entries retain the name, unexpanded
parameters and replacement, physical source location, and exact access spelling.
The catalog includes source-based forced and virtual headers, excludes configured
predefined macros, preserves repeated definitions, and survives later `#undef`.
Every preprocessing entry point starts a fresh catalog. Disabled capture returns
`None`; enabled capture of a source without definitions returns an empty slice.

The final macro environment and normal expansion rules stay unchanged. Consumers
can apply their own declaration-order policy to the historical records. Capture
is independent of file-origin mapping and allocates no catalog when disabled.
Before each copy, it checks a conservative retained-data estimate against
`max_source_bytes`, charging entry storage, owned strings and parameters, and both
path spellings per occurrence even when those paths are shared. This is a data
budget, not a bound on allocator overhead or process memory. Existing source,
token, include, and expansion limits continue to apply.

### Incompatible macro redefinitions

`Config::macro_redefinition_policy` defaults to `MacroRedefinitionPolicy::Strict`,
which preserves the existing error for an incompatible active definition.
`RecordAndReplace` uses the new definition and retains a diagnostic record in
`Preprocessed::macro_redefinitions()`. Equal definitions, inactive directives, and
a new definition after `#undef` create no redefinition record. Optional definition
history still records every successful written definition.

Each record identifies the macro and its current physical/accessed source site.
Configured predefined definitions have no source site. Prior locations are not
inferred. Strict mode returns `None`; compatibility mode returns a possibly empty
slice. Every entry point resets records, including after a failed run. Retained
entry/name/path payload is checked against `max_source_bytes` before allocation,
independently of the optional history budget. Record exhaustion is a preprocessing
error. [Evidence](../../corpus/evidence/macro-redefinitions-2026-09-08/README.md)
covers compiler warnings, expanded output, history, reset, and bounds.
