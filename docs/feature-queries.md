# Preprocessor feature queries

The standalone preprocessor accepts an optional `FeatureQueries` configuration.
Its immutable provider supplies supported names and numeric results; the
preprocessor has no dependency on the target or semantic crates. Without that
configuration, query operators are undefined.

| Operator | GNU 13 | Clang | Argument |
| --- | --- | --- | --- |
| `__has_builtin` | Defined | Defined | GNU expands it; Clang reads a raw identifier |
| `__has_attribute` | Defined | Defined | Expanded identifier; GNU also accepts a namespace |
| `__has_feature` | Undefined | Defined | Raw identifier |
| `__has_extension` | Undefined | Defined | Raw identifier |
| `__has_c_attribute` | Defined | Defined | Expanded identifier or namespace |
| `__has_declspec_attribute` | Undefined | Defined | Expanded identifier |
| `__building_module` | Undefined | Defined | Raw identifier |

Namespace separators must be lexically adjacent. A macro used only as the
separator does not expand there; an enclosing wrapper can prescan its argument.
Clang leaves the trailing lookahead of an unscoped C-attribute name unexpanded.
Malformed arguments diagnose even in `0 && query()`; inactive groups remain
unprocessed. Results retain invocation locations and use the existing byte,
token, and expansion-depth limits. Clang emits an `L` suffix for results greater
than one, including attribute revision dates; GNU leaves them unsuffixed.

`Config::scope_punctuator` independently selects whether `::` is one token. Its
standalone default is false. The facade enables it for Clang and GNU language
modes. Strict GNU modes still recognize physically adjacent colon pairs in query
syntax, but reject pasting them into a single token. Whitespace or substitution
cannot turn two separate colons into a namespace separator. Identity macro
changes do not change the selected language or query rules.

`#undef`, `#define`, and `Config::undefine` can remove or replace an operator.
Each entry point restores configured defaults. `Preprocessed::is_defined`
includes active operators, and final object-macro expansion observes their
current state. Operators are absent from the replacement-macro map. Providers
must be bounded and deterministic; configuration copies share the immutable
catalog through `Arc`, without allocating a per-name cache.

## Frontend capabilities

The facade uses the semantic builtin, GNU-attribute, and Microsoft-attribute
classifiers. It advertises these six Clang language features: `c_alignas`,
`c_alignof`, `c_atomic`, `c_generic_selections`, `c_static_assert`, and
`c_thread_local`. Feature queries require a C11 mode; extension queries also
succeed in C90 modes. Paired surrounding double underscores are accepted query
aliases. The Apple TLS query uses the existing macOS 11 validation baseline;
unversioned Clang Darwin triples can select older targets without TLS. Written
Microsoft attribute names retain their separate, exact spelling
rules.

The MSVC profile advertises checked `align`, `noreturn`, and `noinline` declaration
attributes. Unknown features, ignored attribute contracts, C++/Objective-C
features, sanitizer settings, and modules return zero. C-attribute queries return
zero until C23 attribute syntax and constraints are implemented. A positive
query still requires valid operands and, at a Rust call boundary, a supported ABI.
The catalog measures implemented support rather than every capability of the
native compiler. Compiler version macros remain unchanged in this layer.

Expanding query operands preserve supported `_Pragma` effects, including `once`
when a header is included again. Raw queries and deferred namespace lookahead
retain their argument constraints. Empty, once, system-header, and ignored clang
pragmas work under GNU; Clang also permits diagnostic and message handlers.
Conditional effects follow the configured query dialect even if identity macros
are overridden. The [pragma evidence](../corpus/evidence/query-pragmas-2026-09-08/summary.json)
compares original C compilation before preprocessing: Clang accepts some pack
queries with `-E` but crashes when compiling them. Pack inside a query remains an
explicit unsupported case; a compiler crash is never counted as source rejection.

## Validation

The [operator evidence](../corpus/evidence/remaining-query-operators-2026-09-08/summary.json)
records 5,880 native acceptance/output comparisons and 20 command-line override
cases across four language modes, GCC 13, and five original Clang targets. A
second run uses GCC 14 with explicit `-U__has_feature` and `-U__has_extension` to
constrain its operator set to the modeled GNU 13 environment. Raw availability
is recorded before those overrides. Namespace availability and scope-token
pasting are calibrated independently: GCC 14 accepts scoped attributes in strict
modes while still rejecting pasted `::`. This does not add a GNU 14 profile.

The preprocessing fuzzer independently selects query dialect, trigraph handling,
five comment policies, and scope-token handling. Selector version 3 covers all
40 settings for every seed without consuming any source bytes. Its small catalog
exercises all seven operators and numeric revision results; frontend capability
checks use their own source and compiler comparisons.

## Earlier evidence

The records below describe their original source revisions and coverage.
Subsequent C90 support is documented in [language modes](language-modes.md).

The [recorded differential run](../corpus/evidence/feature-query-operators-2026-09-08/summary.json)
also includes the extracted GNU AArch64 compiler: all 308 case/profile/mode
decisions and successful outputs match. Raw commands, versions, sources, results,
and the capture driver are retained. GNU namespace availability in C11 is recorded
explicitly rather than inferred from the GNU11 result.

## Semantic catalog evidence

The [catalog run](../corpus/evidence/feature-catalog-2026-09-08/summary.json) compares
486 builtin and attribute names across seven profiles and two language modes:
6,804 availability comparisons with no positive claim for a native-unavailable
feature. This measures advertised availability, not complete native feature equality.
Selected families also parse and type-check in ordinary and retained analysis;
invalid operands and attribute arguments still diagnose.

All four default header bindings are byte-identical to the previous recorded
outputs. Seven project source files pass ordinary and retained analysis through
fresh Toucan preprocessing and frozen earlier compiler-preprocessed controls
(28 runs). These use GNU11 analysis; libgit2's original C90 flags remain a separate
gap. The unchanged zstd build script passes all four feature configurations, with
four consumed bindings and 25 identical runtime artifacts.

Seven paired warm CLI measurements per header range from 0.6% faster to 2.1%
slower than `d7ba4de` on this shared host. Configuration and parsing allocate less;
binding generation allocations are unchanged. These measurements establish no
speed improvement or in-process comparison against bindgen.

The archive also records a 120-second AddressSanitizer preprocessing campaign at
`d7ba4de`: 415,264 executions with no finding. It uses the mechanical operators'
small test catalog, independently of this semantic catalog. Leak detection was
disabled in the ptrace environment.

The [combined integration](../corpus/evidence/compiler-query-integration-2026-09-08/summary.json)
also checks 264 feature-selected source pairs across all 44 profile/mode settings,
32 native C compilations, and 3,828 preprocessing/retained-code seed pairs. Eight
reference binding artifacts are unchanged. Its ASan campaign runs 488,116 inputs
in 181 seconds with no findings; source hashes remain unchanged and every seed
covers all 40 preprocessing settings.

The [pragma integration](../corpus/evidence/query-pragmas-root-integration-2026-09-08/summary.json)
checks repeated header inclusion with the semantic catalog across all 44 settings,
reruns native inclusion and identity-override probes, and preserves eight binding
artifacts. Its ASan run processes 264,003 inputs in 121 seconds with no findings
and unchanged source hashes.
