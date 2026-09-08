# C90 comment processing

The standalone preprocessor exposes `Config::line_comments`. Its default remains
`LineComments::Enabled`, which treats `//` as a comment in C99 and later and in GNU
language modes. Selecting another comment policy does not select a C90 parser or
claim support for all C90 semantics.

GCC 13 and Clang 18 differ when processing ISO C90 source without pedantic errors:

| Policy | Ordinary `//` | Initial `6 //**/ 2` | `//` in a pragma payload |
| --- | --- | --- | --- |
| `GnuC90` | Error in active source | Division, yielding 3 | Error |
| `GnuC90Preprocessing` | Error in active source | Division, yielding 3 | Retained slash tokens |
| `ClangC90` | Comment extension | Division, yielding 3 | Comment extension |
| `ClangC90Preprocessing` | Retained slash tokens | Division, yielding 3 | Retained slash tokens |

The preprocessing policies model `-E`; the other policies model source
compilation. These paths differ in the native compilers too. A successful `-E`
run does not establish that the resulting C program is valid.

In Clang source compilation, the first recognized `//` enables line comments for
the rest of that physical file. This includes comments in skipped conditional
groups and unused macro definitions. An included file starts independently, and
the state is discarded when returning to its parent. Before that first comment,
`//**/` remains a slash followed by a block comment. Clang's
[lexer implementation](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Lex/Lexer.cpp)
documents and implements this distinction between source compilation and `-E`.

GCC retains slash tokens in unused macro replacement lists and ignores inactive
ordinary source. Macro expansion cannot create comments. For example, expanding
`#define S /` into `S/` retains two slash tokens under every policy.

## Ordered command-line definitions

Clang's C90 source-compilation state spans its command-line definitions:

```text
-DA=1//first -UA -DB=6//**/2   => B expands to 6
-DB=6//**/2 -DA=1//first -UA   => B expands to 6 / 2
```

Removing `A` does not undo its effect on lexing. A final macro map cannot encode
this ordering. `CommandLineMacroNormalizer` processes each definition in argument
order; callers apply undefinitions separately, then store the normalized text
with `PredefinedMacroMode::Tokens`. This state does not carry into physical files.
The helper bounds its cumulative original input, including removed definitions;
the preprocessor separately bounds the final map, source files, and expansions.
The raw `Config::defines` map is prepared in its documented sorted map order.

## Validation and limits

`toucan_preprocessor/tests/c90_comments.rs` checks skipped groups, includes,
preprocessor reuse, command-line ordering, source budgets, pragmas, and expansion
boundaries. Its native test compares both compilation decisions and preprocessing
outputs against GCC and Clang, including constant-value assertions. Run it with:

```sh
TOUCAN_GCC=gcc TOUCAN_CLANG=clang-18 \
  cargo test -p toucan_preprocessor --test c90_comments -- --include-ignored
```

Existing strict checks for extra directive tokens remain in effect. For example,
GCC may only warn about tokens after `#endif`; the preprocessor diagnoses them.
This layer does not add compiler warning-option emulation, C90 keyword rules,
implicit declarations, or inline-definition ownership.
