# Inline function definitions

A checked body and an externally visible definition are separate facts in C.
For example, a plain `inline` definition in C11 can supply a body for optimization
while an out-of-line call resolves to a definition in another translation unit.
A later `extern` declaration in the same source can change that ownership.

`Declaration.is_definition` continues to report that Toucan checked a body.
`Declaration.function_definition_kind` reports the ownership of its latest body:

| Kind | Meaning |
| --- | --- |
| `External` | Ordinary externally linked definition |
| `Internal` | Definition with translation-unit-local linkage |
| `InlineOnly` | Inline body; an external definition is supplied separately |
| `WeakInline` | Clang inline body that can supply a weak definition when referenced |
| `Superseded` | Earlier GNU extern-inline body replaced by a later body |
| `MicrosoftInline` | Coalesced Microsoft inline definition, possibly omitted when unused |
| `MicrosoftExternInline` | Coalesced Microsoft definition with an explicit extern declaration; an out-of-line instance is emitted |

`Superseded` occurs on retained bodies, not on the canonical declaration. These
kinds describe source and linker ownership, independently of optimization and
explicit weak annotations. They do not predict whether a particular call is
inlined. `SymbolBinding` continues to describe explicit weak annotations.

## Compiler and language rules

On Linux and macOS, C11 and GNU11 use C99 inline ownership: an externally linked
body remains inline-only if all file-scope declarations write `inline` and omit
`extern`. A plain prototype or an explicit extern declaration, including one
written after the body, makes it externally defined. GCC also inherits this
requirement from a non-inline block declaration preceding the first file
declaration. Block declarations after that point, and Clang block declarations,
do not have that effect.

C90 and GNU90 use GNU inline ownership. A written `extern inline` body is
inline-only; a plain inline body owns the external definition. An additional
non-extern inline declaration can force external ownership. GCC considers those
declarations at file scope; Clang also considers block declarations. C90 uses
underscored inline spellings because `inline` remains an ordinary identifier.

The `gnu_inline` attribute selects GNU ownership in all supported language modes.
An attribute written without an inline specifier is ignored. GCC checks attribute
consistency on repeated inline declarations in the same binding scope; a first
block declaration can inherit properties without that diagnostic. Clang inherits
an earlier valid GNU attribute, while a later attribute does not retroactively
change an earlier body. Argument and subject constraints follow the compiler
profile, including Clang's warning-only treatment of the attribute on objects.

Clang's Microsoft target uses coalesced inline definitions. A prior inline
prototype can confer that property on an ordinary body. A later inline prototype
does not retroactively change an ordinary body. An explicit extern declaration,
including a block declaration, requires an emitted out-of-line instance. A valid
GNU-inline attribute selects GNU ownership instead.

GNU extern-inline bodies may precede a replacement definition. GCC and Clang
have different redeclaration constraints; Toucan checks each body in source
order and preserves accepted earlier bodies. This includes Clang's repeated
extern-inline bodies and GCC's rejection once an earlier declaration has already
forced an ordinary definition.

Weak annotations retain their separate symbol-binding metadata. GCC ignores a
weak annotation written on an inline declaration. Clang ignores a first weak
annotation placed after an inline body. A valid weak annotation lets Clang
materialize an otherwise inline-only body as a weak definition; `WeakInline`
distinguishes that case from a body that cannot supply an external definition.
Microsoft coalescing keeps its existing kind and carries the weak binding
separately. An `always_inline` body in LLVM's `available_externally` form still
contributes no linker definition and remains `InlineOnly`.

## Retained source and resource limits

`FunctionBody::definition_kind()` gives each body's final ownership.
`Entity::body()` returns the latest body; earlier bodies remain in `bodies()` and
on their original `DeclarationSite::body()` links. Their statements, parameter
scopes, and source occurrences remain distinct.

`CheckedCode::function_inline_sites()` records written inline keywords,
GNU-inline attributes, and explicit extern spelling. It includes ordinary
prototypes participating in that function's history. Source ranges use the
original preprocessed input, including parser-adapter mappings. The record does
not assert an inherited optimization hint.

Ordinary analysis keeps histories only for names with inline syntax. Collection
is bounded by four million syntax visits, 512 traversal levels, 128 declarator
levels, and 65,536 names; histories contain at most 262,144 declarations. Retained
source records are additionally charged to the caller's node, edge, and payload
budgets before allocation. On the measured 64-bit Rust host, the added enum fits
existing padding: `Declaration` remains 136 bytes, `FunctionBody` 56 bytes,
`DeclarationSite` 256 bytes, and `Type` 40 bytes.

## Validation and scope

The source fixture contains native acceptance and symbol expectations for
redeclarations, scope, GNU attributes, ignored subjects, replacement bodies,
and Microsoft coalescing. The regular test replays those expectations with
ordinary and retained analysis across all supported profiles and modes. An
ignored test compiles the same cases with native GCC and all Clang cross targets.
Separate tests check unused Microsoft emission and execute linked translation
units under UBSan, proving that inline-only calls resolve to a separate definition.

[`probe_inline_ownership.py`](../scripts/probe_inline_ownership.py) reproduces the
recorded native compiler commands, diagnostics, LLVM definitions, and GNU symbol
tables. It records the fixture checksum and exits unsuccessfully on a mismatch.

Binding generation retains its existing definition filtering. Inline binding emission,
wrapper generation, optimization, and machine-code generation remain separate
work. Compiler flags such as
`-fgnu89-inline`, `-fkeep-inline-functions`, and `-fms-extensions` do not silently
select a new profile through these APIs.

[Recorded evidence](../corpus/evidence/inline-ownership-2026-09-08/summary.json)
includes 3,136 core symbol/admission checks, 832 weak/forced-inline checks, eight
UBSan-linked executions, and a 120-second sanitizer campaign with 6,834 inputs.
All seven real-project translation units pass both preprocessing routes with
ordinary/retained parity. Paired process benchmarks preserve complete normalized
declarations and report raw timing variability alongside the measured medians.

The [integration checks](../corpus/evidence/inline-integration-2026-09-08/summary.json)
repeat native inline, weak, allocation, and prefetch tests on the combined source.
Microsoft declaration alignment had already grown `Entity` to 88 bytes; this
layer preserves that size and the other measured declaration/body sizes. The
original evidence remains tied to its earlier source snapshot.

The [root integration](../corpus/evidence/inline-root-integration-2026-09-08/summary.json)
passes 781 workspace tests and 3,916 checked seed pairs across all 44 profile/mode
settings. Eight existing binding artifacts are byte-identical. Its ASan campaign
completes 19,662 inputs in 181 seconds with no findings and unchanged source.
Distinguishing inputs verify that the frozen baseline, core inline release, and
weak-inline release expose their expected different ownership metadata.
