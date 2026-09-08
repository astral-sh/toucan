# C11 and GNU11

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

The build-script adapter accepts `.clang_arg("-std=c11")` and
`.clang_arg("-std=gnu11")`. The last standard option wins. A compiler profile
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

Inspection and binding reports add `language_mode`, spelled `c11` or `gnu11`.
The experimental inspection versions remain 3 (declarations) and 5 (checked
code). Older serialized compiler profiles default a missing field to GNU11.
Declaration debug hashes change because the unit records this explicit field;
generated Rust binding bytes are independent of the metadata.

C90, C99, C17, C23 and corresponding GNU modes are not modeled. Unsupported
standard flags remain errors in the CLI and build-script adapter. The source
audit records its selected analysis mode and leaves unmodeled compiler flags
visible: libgit2's current `-std=c90` is not relabeled C11. C90 support and GNU's
omitted-middle conditional expression (`x ?: y`) are tracked conformance work.
Assembly bodies on Windows retain their existing explicit unsupported diagnostic.

## Validation

The [recorded evidence](../corpus/evidence/language-modes-2026-09-08.json.gz) includes
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

The [integration checks](../corpus/evidence/language-modes-integration-2026-09-08/summary.json)
cover the merged BMI, binary128, and vector layers: all 1,050 seed/profile/mode
pairs agree between ordinary and retained analysis (747 accepted, 303 matching
diagnostics). The generated C11 bindings also pass with Rust 1.64. Campaign seeds
cover both modes for every profile and both trigraph settings for preprocessing.
