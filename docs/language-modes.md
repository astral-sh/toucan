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

## Validation

The semantic [language-mode](../crates/toucan_semantic/tests/language_modes.rs),
[C90](../crates/toucan_semantic/tests/c90.rs), and
[C99/C17](../crates/toucan_semantic/tests/c99_c17.rs) tests cover source admission,
predefines, compiler profiles, and ordinary/retained agreement. Their native
oracles compare GCC and Clang decisions. The facade's
[C90](../crates/toucan/tests/c90.rs) and
[C99/C17](../crates/toucan/tests/c99_c17.rs) tests also compile generated bindings
and exercise C/Rust calls.

Use the [conformance gate](conformance.md) for the pinned program corpus and
[validation guide](validation.md) for current commands and result boundaries.
Earlier mode-specific compiler, fuzz, and performance observations remain in
the [historical archive](validation.md#historical-results).
