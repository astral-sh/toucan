# toucan_bindgen

A C binding builder for existing Rust build scripts, backed by Toucan's Rust
frontend. It does not require libclang or invoke a C compiler.

This adapter is experimental and implements a subset of bindgen's API. The pinned
zstd-sys 2.0.16 build script uses it unchanged: replace its build dependency with
the local package, keeping the dependency name `bindgen`:

```toml
[build-dependencies.bindgen]
package = "toucan_bindgen"
path = "../toucan/crates/toucan_bindgen"
features = ["runtime"]
default-features = false
```

The `runtime` feature accepts the existing manifest's feature selection. Both
configurations use Toucan's frontend. Building the adapter requires Rust 1.96;
generated declarations can target Rust 1.64 or later.

`RustTarget` accepts `"1.64".parse()?` and `"1.64.0".parse()?` in existing
`rust_target` calls. Stable patch releases select the same language features.
Parsing returns `std::io::Error` with `InvalidInput` for invalid or unsupported
targets. Targets must be stable Rust 1.64 or newer, with a minor number no greater
than 65535. Nightly targets are explicitly unsupported. The default remains Rust 1.64.

The [target parsing capture](../../corpus/evidence/rust-target-parse-2026-09-09.json.gz)
checks 47 strings against bindgen, preserves the existing version-policy
differences, and verifies identical generated output for parsed and constructed
stable targets.

## Supported build scripts

`Builder` supports ordered `header` calls, `clang_arg`/`clang_args`, `use_core`,
`size_t_is_usize`, `rust_target`, `layout_tests`, `raw_line`,
`blocklist_function`, `blocklist_type`, `rustified_enum`, and `prepend_enum_name`.
Integer macro options include `default_macro_constant_type` and `fit_macro_constants`.
Generation returns bindings
with `Display`, `write`, `write_to_file`, and a `report()` containing omitted macros and
dependencies. Compile-time size and alignment assertions remain enabled when
runtime layout tests are disabled.

Function blocklists and Rust enum selectors accept exact identifiers or prefixes
ending in `.*`, with optional anchors. Other regular expressions produce errors.
Rust enum selection uses the C tag or the first typedef naming an anonymous
enum; later aliases do not match. See [enum selection](../../docs/enum-selection.md)
for anonymous constants and generated nested-name boundaries.
Integer enum constants include the enum name by default (`Mode_VALUE`);
`prepend_enum_name(false)` keeps `VALUE`. Named Rust enums expose their variants
without extra global integer constants. Truly anonymous Rust enums also expose
enum-typed globals. See [enum constant names](../../docs/enum-constant-names.md)
for macro collisions and the core library's separate default policy.
Type blocklists omit matching definitions and preserve their
uses under the names listed in `report().blocked_types`. Supply those Rust types
with imports or `raw_line`. The blocklist is not recursive: an independently
named dependency can still be emitted. Blocked anonymous enum typedefs also omit
their enum constants; a blocked alias of a named enum does not block that enum.
See [external types](../../docs/external-types.md) for layout and ABI contracts.
These boundaries are not general bindgen API compatibility.

`Formatter::Rustfmt` is the default. Formatting runs when bindings are displayed
or written; `generate()` does not start rustfmt. `Formatter::None` writes the
unformatted source. `with_rustfmt` overrides `RUSTFMT` and the default executable
lookup on `PATH`. `rustfmt_configuration_file` sets a configuration path and
enables rustfmt. Raw lines appear after the banner and before declarations, with
their bytes preserved outside formatting. Writer errors propagate; formatter
failures report a diagnostic and use unformatted declarations. See
[formatting and output](../../docs/bindgen-formatting.md) for the pinned behavior
comparison and supported formatter names.

`clang_version()` identifies the Toucan package in `full` and returns `None` in
`parsed`: it does not load libclang or report the emulated Clang semantic profile
as an installed compiler version.

`generate_comments(true)` is the default. It attaches C documentation comments to
generated declarations, named fields, and enum variants as escaped Rust `#[doc]`
attributes. `generate_comments(false)` disables capture and emission. The arguments
`-fparse-all-comments` and `-fretain-comments-from-system-headers` additionally
enable ordinary comments and system-header comments. See
[documentation emission](../../docs/documentation-emission.md) for macro locations,
redeclarations, selection, and the native comparison.

The `derive_copy`, `derive_debug`, `derive_default`, `derive_eq`, and
`derive_partialeq` options request traits where the emitted storage supports them.
The Builder requests `Copy` and `Debug` by default. Explicit overrides take
precedence, and ineligible records omit the corresponding trait. A generated
`Default` implementation requires a valid zero representation; a nonzero Rust enum inside a struct
prevents a generated Default implementation. See [binding traits](../../docs/binding-derives.md)
for unions, non-Copy members, callbacks, and the measured bindgen differences.

Toucan additionally provides `dll_import_library(pattern, library)` for checked
Microsoft DLL imports. It accepts the same exact-name or trailing `.*` patterns
as the blocklists. Exact names win, then the longest prefix; repeating a pattern
replaces its library. It attaches Rust's `#[link]` to the matching imported
symbols' foreign blocks. Selected imported data needs a matching rule. Other
symbols remain outside that annotation. See [DLL storage](../../docs/msvc-dll-storage.md)
for multiple-library scope, linker search paths, and native validation.

The default representation uses `usize` for compatible `size_t`, unsigned types
for nonnegative integer macros, core paths, and Rust 1.64 syntax. Select enum
variants explicitly: Rust enums cannot represent arbitrary integer values.
Unsupported C syntax and unproved Rust calling ABIs remain generation errors.

Bare function typedefs use bindgen's nullable callback representation. For
`typedef int Callback(int);`, the Rust alias is `Option<unsafe extern "C" fn(...)>`.
A C `Callback *` uses that alias directly; `Callback **` becomes a raw pointer
to the alias. This preserves typed `None` values in existing Rust consumers.
See [function typedef bindings](../../docs/function-typedef-bindings.md) for
alias chains, nested callbacks, external replacements and native evidence.

## Targets and arguments

Cargo's `TARGET` selects the target. An explicit `--target` or `-target` overrides
it. Outside a build script, the supported native host is the default. Every target
uses the Clang semantic profile, including on Linux. Both GNU and musl Linux
triples are supported on x86-64 and AArch64; provide the selected libc
[sysroot and compiler resource headers](../../docs/musl.md).

Supported arguments are `-I`, `-D`, `-U`, `-isystem`, `-include`, `--sysroot`,
`-isysroot`, `-x c`, `-std=c11`, `-std=gnu11`, and trigraph overrides. Unknown options fail generation;
ABI-changing flags are never silently ignored. System include directories follow
ordinary include directories. A sysroot adds `usr/include` and the selected Linux
multiarch directory. Target headers and compiler resource directories must be
supplied explicitly; no host compiler or SDK discovery occurs.

The adapter defaults to GNU11. [C90 through C17 and their GNU modes](../../docs/language-modes.md)
select syntax, declaration rules, and predefined macros without enabling pedantic
diagnostics. The documented compiler aliases, including C89, C9x, and C18, select
the corresponding canonical mode. C23 and GNU23 remain explicit errors.

The adapter reads `BINDGEN_EXTRA_CLANG_ARGS` with shell quoting. Target-specific
forms take precedence, first using the Cargo target spelling, then replacing its
hyphens with underscores. Explicit builder arguments precede these environment
arguments. It captures one UTC timestamp per generation; `SOURCE_DATE_EPOCH`
selects a reproducible timestamp. Invalid values produce errors.

## Validation

`scripts/verify_bindgen_builder.py` compares four real zstd consumer feature
configurations against the original checked-in bindings. It changes only the
zstd-sys Cargo dependency, checks every upstream source hash, and requires rustc
dependency files to name the generated output. Compression, streaming,
dictionaries, experimental APIs, and shared thread pools must return identical
results and artifact bytes. The generated dependency graph must contain no
bindgen, clang-sys, or libloading packages. Native corpus CI runs this gate on
Linux. The macOS validation workflow covers Apple Silicon and includes Intel
only when requested.

The [saved Linux run](../../corpus/evidence/bindgen-builder-2026-09-08/summary.json)
records all four configurations and 25 byte-identical runtime artifacts. It
includes the consumed generated files and unchanged build-script hash.
The [macro-history refresh](../../corpus/evidence/builder-corpus-dependencies-2026-09-09/README.md)
also passes all four profiles and 25 artifact comparisons. Initial metadata
resolution fetches the adapter's dependencies; subsequent builds remain locked
and offline.

For complete application paths, `scripts/verify_astral_builder.py` builds pinned
ty and uv through zstd-sys's unchanged build script and compares their compression
tests and CLI behavior. The [consumer guide](../../docs/astral-consumers.md#run-the-unchanged-bindgen-build-script)
describes the manifest-only activation and source/lock/artifact checks.

## Physical header selection and generated names

`allowlist_file(pattern)` accepts Rust regular expressions anchored to the whole
compiler-visible access name, before diagnostic `#line` remapping. Repeated patterns select their union. Logical `#line` names
have no effect. Selection keeps tag and ordinary namespaces separate; required
type dependencies are included. A nonmatching pattern selects no declarations.
The adapter captures a lightweight declaration/file catalog for this policy,
without retaining checked expressions, bodies, or initializers.

`parse_callbacks(Box<dyn callbacks::ParseCallbacks>)` supports
`generated_name_override(ItemInfo)`. Callbacks run synchronously on the calling
thread and may contain `Rc` or `RefCell`; they need not be `Send` or `Sync`.
Callbacks are tried newest first until one returns a name, before file filtering.
Compatible redeclarations are visited in source order. Functions retain the first
selected occurrence's name. Objects may expose several selected names, each
using its first selected occurrence. The original C symbol remains their
`link_name`. Only functions and externally linked objects use this callback;
inline function candidates are excluded before invocation, while static function
prototypes still invoke it. Generated names must be ASCII identifiers, use
Toucan's existing reserved-identifier escaping, and cannot collide with another
selected Rust value.

Macro values follow bindgen 0.72.1's written definition order. The first parsed
definition supplies output; later definitions update values used by subsequent
macros. `#undef` does not erase that parsed-value context. Configured predefined
macros and enum constants are not seeded into it. File selection uses the first
parsed definition's accessed header, after updating the context. With callbacks,
names that are function-like in the final active environment are skipped.

Integer values wrap as i64, independently of C suffixes. The default
`MacroTypeVariation::Unsigned` uses u32/u64 for nonnegative values and i32/i64 for
negative values. `Signed` uses i32/i64; `fit_macro_constants(true)` also permits
8- and 16-bit integers. These options do not change character, string, or f64
representations. Unsupported expressions and out-of-range characters appear in
`report().skipped_macros`; resource exhaustion fails generation. Reports mark this
evaluation as `MacroEvaluation::Provided` and retain accepted incompatible
redefinitions with their physical locations. The core library's C constant
evaluation remains a separate API. See [macro values](../../docs/macro-value-compatibility.md)
for the grammar, bounds, and differential evidence. `push_macro` and `pop_macro`
remain unsupported.

The Builder emits ordinary externally linked C function definitions and excludes
inline candidates. Its default path uses compact declaration-time inline facts,
without enabling origin capture. A plain body followed by a later inline prototype
remains eligible; an inline body, including a replaced GNU body, is excluded.
The core emitter retains its separate opt-in definition and inline policies.
See [default function selection](../../docs/bindgen-functions.md) for native evidence.
Enum naming/derive/comment/format policies are independent APIs and are not added
by file selection or callbacks.

Relative main names such as `root.h` and included names such as `./a.h` remain
available to file patterns. Ordered headers use native `-include` processing for
all but the last main header. Dependencies keep canonical filesystem identities;
symlink-relative lookup and compiler-visible access spelling follow the selected
compiler's file-name rules.

## Scalar object constants

Supported scalar object initializers emit Rust constants with their declared C
primitive or typedef type. The first declaration in a selected header controls
constant-versus-extern emission. Internal objects use their original name;
external objects retain name callback behavior. See [scalar object bindings](../../docs/static-object-bindings.md)
for conversion, redeclaration, string/const-read boundaries, and native value checks.

Direct narrow string object initializers project their terminated C-string prefix.
Captured literal bytes and C array bounds remain available independently. Unterminated
arrays produce a diagnostic. See [string object bindings](../../docs/string-object-bindings.md)
for embedded NULs, padding, UTF-8, preserved storage bindings, and bounded byte checks.

Direct 128-bit integer object constants retain complete values on Rust 1.64,
independently of the Rust 1.78 requirement for C 128-bit storage and call types.
See [128-bit constants](../../docs/int128-object-bindings.md) for native validation
and the documented difference from bindgen's truncated values.

## Name allowlists

`allowlist_type`, `allowlist_function`, and `allowlist_var` accept Rust regular
expressions anchored to the entire name. Repeated patterns and all file/name
categories select their union. A nonmatching pattern selects no roots; invalid
expressions fail generation. Required types are included recursively.

Type patterns match typedefs and lexical tag names such as `Outer_Inner`.
Named incomplete tags introduced by record members keep their file-scope names:
`struct Owner { struct T *data; };` exposes `T`, while a definition inside
`Owner` exposes `Owner_T`. An unrelated `typedef int T` keeps the qualified
opaque name `Owner_T` to avoid a Rust type-name collision. Prototype-scope tags
remain separate types, and incomplete records expose no public storage fields.
Function patterns match functions only, even when a C tag shares that spelling.
Variable patterns match objects and written object macros. An enumerator of an
anonymous top-level enum without a typedef selects that whole enum; enumerators
of named, nested, or typedef-named enums do not independently select it.

Function and external-object patterns match each occurrence's
`generated_name_override` result. File/name selection happens before choosing
the first selected occurrence's name and object initializer for each generated
object name. Several selected object names can share one C symbol while keeping
separate checked initializers. Macro patterns
match original C names and keep the ordered evaluation context, including values
from excluded definitions.

Selected static function prototypes and blocklisted functions still contribute
their referenced types. Inline definitions excluded by the adapter do not
become name roots. Reached blocklisted type definitions can contribute their
own type dependencies while their definitions remain caller-supplied. This
behavior uses the core's opt-in `BindingSelection::retain_type_dependencies`;
ordinary core and file-only selection retain their existing policies.

See [multiple object names](../../docs/multiple-object-names.md) for the optional
core projection map and native selection/linkage evidence.
