# C language modes

Toucan defaults to GNU11. Select C11 when the build uses `-std=c11`:

```sh
toucan check api.h --std c11 --compiler clang
```

```rust
use toucan::{Compiler, CompilerProfile, Config, LanguageMode, Target};

let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang)?
    .with_language_mode(LanguageMode::C11);
let config = Config::with_profile(profile);
```

The CLI and build-script adapter accept C90, GNU90, C99, GNU99, C11, GNU11,
C17, and GNU17. For example,
use `.clang_arg("-std=c90")` for a C90 build. `c89` and `iso9899:1990` are aliases
for C90; `gnu89` is an alias for GNU90. The last standard option wins. A compiler profile
owns the mode; checked translation units retain it so constant-evaluation
fragments and later type queries use the same keywords as the original source.

C11 permits `asm` and `typeof` as identifiers. GNU11 reserves these spellings for
assembly and type-query extensions. Underscored alternatives such as `__asm__`
and `__typeof__` remain available in both modes. C11 does **not** imply
`-pedantic-errors`: supported compiler extensions, including statement
expressions and `__auto_type`, remain available.

## Preprocessing and option order

C11 defines `__STRICT_ANSI__=1` and removes bare `linux` and `unix` macros on
Linux. Both modes preserve underscored platform macros and
`__STDC_VERSION__=201112L`. Clang's Windows Microsoft-compatibility profile omits
`__STRICT_ANSI__` in both modes, matching its compiler driver.

C11 enables trigraph replacement before line splicing. GNU11 disables it.
Clang's Windows profile defaults to disabled in both modes. Embedders can set
`config.preprocessor.trigraphs` explicitly. The CLI accepts `--trigraphs` or
`--trigraphs=false`; the Clang adapter accepts `-trigraphs`, `-ftrigraphs`, and
`-fno-trigraphs`. A later standard option resets an earlier trigraph override;
a later trigraph override wins.

All `-D` and `-U` operations run after the final profile predefines, preserving
their occurrence order. Changing `__STRICT_ANSI__` explicitly does not change
parser mode. Replacement bodies stop at the first newline and remove comments.
Clang applies enabled trigraph replacement to those bodies; GNU does not.
Definitions cannot splice into another definition or forced include.

C90 and GNU90 omit `__STDC_VERSION__`. They define `__GNUC_GNU_INLINE__` where
C99 and later modes define `__GNUC_STDC_INLINE__`; Windows defines neither. GCC C90 omits
`__STDC_UTF_16__` and `__STDC_UTF_32__`, while Clang keeps both macros. ISO C90 has
the same strict-mode platform macros and trigraph defaults as ISO C11.

ISO C90 [comment handling](c90-comments.md) follows the selected compiler.
Compilation and `-E` differ in the native compilers. The facade and build-script
adapter select compilation behavior; the CLI's `preprocess` command selects
preprocessing-only behavior. Clang C90 compilation normalizes `-D` operations in
argument order, including a definition later removed by `-U`, before storing the
final macro map. This preserves Clang's comment-extension state.

The standalone preprocessor has no compiler dependency. Its default remains
C11 trigraph replacement with caller-supplied replacement tokens. Its
`PredefinedMacroMode` option selects raw tokens, GNU command-line definitions, or
Clang command-line definitions. Physical files and forced includes retain their
ordinary translation phases and source locations.

These defaults and driver-order rules are confirmed against GCC 13.3 and Clang
18.1.3, including GNU ARM and Clang's five targets. Primary implementation:
[Clang language defaults](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Frontend/CompilerInvocation.cpp),
[predefines and command-line macros](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Frontend/InitPreprocessor.cpp),
and [driver option order](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Driver/ToolChains/Clang.cpp).

## Coverage and remaining modes

Inspection and binding reports retain the selected `language_mode` using its
canonical spelling, such as `c99`, `gnu99`, `c17`, or `gnu17`.
The experimental inspection versions remain 3 (declarations) and 5 (checked
code). Older serialized compiler profiles default a missing field to GNU11.
Declaration debug hashes change because the unit records this explicit field;
generated Rust binding bytes are independent of the metadata.

C90 uses `inline` as an identifier; GNU90 reserves it. Both permit `restrict` as
an identifier and retain the underscored alternatives `__inline__` and
`__restrict__`. UTF-prefixed literals require C11 or later, except that the GNU compiler profile
also accepts them in GNU99. Clang rejects these literals in both C99 modes. The compiler profiles
retain non-pedantic extensions such as `_Atomic`, `_Generic`, compound literals,
designated initializers, and variable-length arrays. GCC rejects declarations
in a C90 `for` initializer; Clang accepts them as an extension.

C90 implicit-int syntax covers object, typedef, function, qualified parameter,
and identifier-list parameter declarations. Missing identifier-list parameter
declarations produce `int` objects in source order. Retained analysis records
their real identifier-list occurrences and parameter-entry types without
inventing explicit declaration syntax.

A direct call to an undeclared ordinary name introduces `extern int name()` in
the innermost scope, with default argument promotions. Parenthesized undeclared
names remain errors. Later declarations are checked for compatible types and
linkage. Each implied declaration has an `ImplicitFunction` retained occurrence,
and declarations in different blocks share the externally linked entity.
Clang's Microsoft compatibility profile also accepts a later written `static`
function declaration while preserving its earlier external linkage, including
after an implied declaration. Native LLVM output verifies that the definition
remains externally visible; retained entity and declaration linkage agree.

Implicit declarations of library builtins remain unsupported: compilers can give
`malloc` a pointer-returning prototype even without a header. A conservative union
of ordinary library names from pinned
[Clang](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/include/clang/Basic/Builtins.def)
and [GCC](https://github.com/gcc-mirror/gcc/blob/releases/gcc-13.3.0/gcc/builtins.def)
catalogs, plus reserved names, requires a supplied declaration. This union also
includes names disabled in individual profiles. Incompatible out-of-scope
function declarations remain an explicit unsupported warning-recovery case.
[`generate_implicit_library_names.py`](../scripts/generate_implicit_library_names.py)
reproduces the 1,084-name guard from checksum-verified upstream catalog files.

C90 decimal integer constants consider unsigned `long` before `long long`, as
the native profiles do. Values above the supported integer range and signed
MS-compatible `LL` overflow recovery remain errors. C11's existing rejection of
implicit declarations is unchanged, including cases GCC only warns about by
default. [Inline-definition ownership](inline-functions.md) follows the selected
language mode and compiler, including GNU attributes, later declarations, and
Microsoft coalescing. Binding generation retains its existing definition filter.

C23 and GNU23 are not modeled. Unsupported
standard flags remain errors in the CLI and build-script adapter. The source
audit records its selected analysis mode and leaves unmodeled compiler flags
visible. Libgit2's original `-std=c90` now selects C90. GNU's omitted-middle
conditional expression (`x ?: y`) remains tracked conformance work.
Assembly bodies on Windows retain their existing explicit unsupported diagnostic.

## C99 and C17

C99 and GNU99 reserve `inline` and `restrict`, enable ordinary line comments and
`for` declarations, and use the modern inline-definition rules. They define
`__STDC_VERSION__=199901L`. Their decimal integer candidates follow the C11
profile, including the existing rejection of oversized unsuffixed decimal
constants outside C90; native warning recovery is not silently applied.

C17 and GNU17 define `__STDC_VERSION__=201710L`. They share the supported C11
syntax and semantic rules. The pinned GCC manual states that its C17 corrections
also apply in C11 and only the version macro differs; Clang's pinned language
standard flags retain the C99 and C11 rules. These modes select a compiler
profile and supported feature set, rather than claiming complete ISO conformance.

GCC enables UTF-prefixed literals and `__STDC_UTF_16__`/`__STDC_UTF_32__` in GNU99;
it omits the macros and rejects those literals in ISO C99. Clang defines the UTF
macros in C99 while rejecting UTF-prefixed literals until C11. Both compiler
profiles retain their non-pedantic C11 extensions in C99, including `_Atomic`,
`_Generic`, `_Static_assert`, alignment, and thread-local storage. Clang's
`__has_feature` reports these C11 features only from C11 onward;
`__has_extension` reports the supported extensions in the earlier modes.

Aliases shared by the modeled compilers include `c9x`, `iso9899:1999`, and
`iso9899:199x` for C99; `gnu9x` for GNU99; `c18`, `iso9899:2017`, and
`iso9899:2018` for C17; and `gnu18` for GNU17. The C11 aliases `c1x`,
`iso9899:2011`, and `gnu1x` are also accepted. Unsupported standard options remain
visible in source audits and are rejected by the CLI and build-script adapter.

Primary sources: [GCC 13.3 C standards](https://github.com/gcc-mirror/gcc/blob/releases/gcc-13.3.0/gcc/doc/standards.texi),
[Clang 18.1.3 standards](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/include/clang/Basic/LangStandards.def),
and [Clang feature predicates](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/include/clang/Basic/Features.def).

### C99/C17 validation

The [mode evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/c99-c17-2026-09-08/summary.json) records
2,944 native syntax/predefine/query observations, 174 option controls, and 1,176
inline-symbol checks for the new modes. Focused tests compare 800 source
admission decisions with GCC 13.3 and Clang 18.1.3. Eight linked C/Rust probes
exercise all four new modes with both native compilers and Rust 1.64, including
inline helpers, `restrict`, loops, constants, and struct arguments and returns.

All eight routes through the 220-program corpus accept 219 programs and pass
all 211 strict-C11 control cases. The remaining exploratory difference is an
assignment that discards `const`; the native compilers reject it in pedantic
C11. No tool or oracle-pipeline failures occurred. The archive preserves each
original source, command, diagnostic, and preprocessed input.

The workspace passes 785 tests, with 207 opt-in tests ignored in that run. The
native tests run separately. Both Python harness suites pass (32 and 27 tests).
All seven real-project translation units pass both preprocessing routes with
ordinary/retained parity. Seven paired benchmark rounds on those inputs retain
byte-identical complete declarations; median time ratios span 0.960–1.021.
These measurements ran on a shared host alongside other validation and do not
establish a speedup. Baseline and candidate binaries use separate build caches
and pass a distinguishing GNU99 literal control before measurement.

The [sanitizer evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/fuzz/evidence/c99-c17-2026-09-08) includes the first
expanded-corpus replay and a longer mutation campaign. The final run executes
22,764 inputs in 301 seconds with 624 MiB peak RSS, 792 new corpus units, no
artifacts, and unchanged source hashes. All 88 compiler/mode settings are seeded
without changing source bytes. These bounded runs include rejected programs and
do not establish complete conformance or safety. LeakSanitizer remains disabled
in this ptrace environment.

## C90/GNU90 validation

The [admission evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/c90-modes-2026-09-08/summary.json)
records native compiler decisions, full commands, diagnostics, and source hashes.
Focused tests compare 228 decisions across both C90 modes, GCC 13.3, and Clang
18.1.3 targeting Linux, macOS, and Windows. A Windows LLVM emission check verifies
the external linkage described above. Separate generated-binding tests execute
C calls and struct round trips with both native C compilers and Rust 1.64.0.

All seven untouched source-audit translation units pass through both preprocessing
routes, with ordinary and retained results agreeing in all 14 pairs. Both libgit2
inputs use their original C90 build flag. This is a functional source audit;
its individual timing observations are not a comparative performance claim.
The workspace suite passes 690 tests, with 185 opt-in tests ignored in that run.
The focused native checks run separately; the changed crates pass Clippy.

The [checked-analysis fuzz campaign](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/fuzz/evidence/c90-modes-2026-09-08/evidence.json.gz)
passes 31,857 inputs in 181 seconds with AddressSanitizer, no artifacts, and
543 MiB peak RSS. The initial archive covers all seven profiles from its base
revision and all four modes for each of 77 seed files, preserving every original
input byte. Source hashes remain unchanged during the run. LeakSanitizer is
disabled in this ptrace environment; these results are a bounded campaign.

### Integration with all eleven profiles

The [integration evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/c90-integration-2026-09-08/summary.json)
records the C90 layer combined with Microsoft integer and calling-convention
syntax, retained `_Noreturn` metadata, and all eleven compiler profiles. The
workspace passes 732 tests, with 192 opt-in tests ignored in that run; focused
native checks run separately. The native C90 table covers 320 decisions, including
Microsoft syntax, plus the Windows external-linkage LLVM check. The 56-observation
feature-query table confirms that GCC's `gnu::` attribute namespace is enabled
in GNU90 and GNU11 and returns zero in ISO modes; Clang rejects scoped query
arguments. The differential CLI table remains at 180/184 because empty scalar
initializers are reserved for the following implementation layer.

All 117 semantic fuzzer seed files retain their exact bytes across 44
profile/mode settings. The preprocessing fuzzer keeps its independent 20 settings
for each of six seeds. A checked-analysis AddressSanitizer campaign executes
12,583 inputs in 121 seconds with 592 MiB peak RSS, no artifacts, and unchanged
source hashes. Its initial archive contains every profile/mode setting for all
82 checked seeds, plus the invalid-UTF-8 input. LeakSanitizer remains disabled.
The earlier evidence directories are preserved without changes.

## Earlier C11/GNU11 validation

The [recorded evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/language-modes-2026-09-08.json.gz) includes
compiler versions, exact commands, source/binary hashes, and raw observations.
The workspace suite passed 630 tests; the parser and semantic native suite passed
533 with no ignored tests. Focused mode tests cover ordinary/retained parity,
all seven profiles, command-line order, source locations, and Rust 1.64 output.
AddressSanitizer replay passed 980 seed/profile/mode pairs (695 accepted, 285
matching diagnostics), with 459,416 KiB peak RSS.

Seven untouched translation units passed both preprocessing routes, ordinary and
retained, on both revisions: 56 completed controls. Complete declaration output
matched after removing only the new mode metadata. Another 32 controls passed
for C11-mode complex.h, FFTW, and LAPACKE headers with GCC and Clang.

Release binding measurements compare this layer with commit `6427b5b`, using the
system allocator on the same shared Linux host. Seven alternating observations
per project ran on CPU 2 while independent builds and native tests ran elsewhere.

| Header | Baseline median | Mode layer median | Ratio |
| --- | ---: | ---: | ---: |
| zlib | 48.58 ms | 49.32 ms | 1.015 |
| sqlite | 173.85 ms | 174.30 ms | 1.003 |
| zstd | 16.43 ms | 16.53 ms | 1.006 |
| libgit2 | 391.00 ms | 391.08 ms | 1.000 |

These observations do not establish a speed improvement. Generated Rust hashes,
preprocessing/analysis allocation counts and bytes, and binding allocation counts
and bytes match the baseline on all four headers. `TranslationUnit` remains
176 bytes on this host; `CompilerProfile` grows from 2 to 3 bytes. Five alternating
ordinary/retained semantic observations on those headers span ratios 0.968–1.027;
VLA and inferred-type controls also retain identical allocation counts.

The [integration checks](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/language-modes-integration-2026-09-08/summary.json)
cover the merged BMI, binary128, and vector layers: all 1,050 seed/profile/mode
pairs agree between ordinary and retained analysis (747 accepted, 303 matching
diagnostics). The generated C11 bindings also pass with Rust 1.64. Campaign seeds
cover both modes for every profile and both trigraph settings for preprocessing.

The [combined integration](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/c90-root-integration-2026-09-08/summary.json)
also includes Microsoft declaration attributes and literal-query optimization.
It checks 3,696 ordinary/retained seed pairs across all 44 profile/mode settings,
keeps eight existing binding artifacts byte-identical, and reruns native C90 FFI
with Rust 1.64. The original archives retain their own source revisions.

The [220-program corpus rerun](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/conformance-a3256d6/summary.json)
at `a3256d6` covers all four modes through both GCC and Clang preprocessing.
The existing strict-C11 control gate passes in every route: 211 programs in each
C11 mode, 189 in C90, and 210 in GNU90 after intersection with that mode’s native
acceptance. This does not establish pedantic C90 conformance. The only exploratory
difference is a pointer assignment that discards `const`; both native compilers
reject it under pedantic C11. No tool or oracle-pipeline failures occurred.
