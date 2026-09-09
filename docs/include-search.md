# Include search roots

`PreprocessorConfig.include_dirs` contains regular search roots; the new
`system_include_dirs` contains system roots. Regular roots precede system roots.
The Builder maps `-I` to regular roots and `-isystem` and its configured sysroot
headers to system roots.

At the start of each preprocessing run, roots are resolved by physical directory
identity. Duplicate directories are searched once. If a directory appears in both
groups, its system entry wins; the first spelling in the winning group is used.
The configured vectors are not rewritten. Unix uses directory device/inode;
other hosts use canonical directory paths. The latter does not claim equivalence
for distinct canonical names that the host filesystem treats as one directory.
Missing/non-directory roots are omitted from the normalized multi-root list.

`#include_next` and `__has_include_next` use this per-run search order. Normalizing
again on reuse lets a changed symlink select its new directory. Clang's quoted
local include preserves its parent's search position; GNU restarts at the
beginning. The compiler profile controls this rule even after a source file
undefines `__clang__`.

## Initial system class

When file-origin capture is enabled, `FileMapping::is_system_include()` and
`FileOrigins::source_is_system_include(offset)` expose the initial system class
of an inclusion. It is true for a system root, for built-in resource headers,
and for descendants of a system inclusion, including children found through a
regular root. A later separate inclusion starts from its own parent/root class.
The main input is not classified as system merely because its directory is a
system root.

This metadata describes the include boundary. It does not model subsequent
`#pragma GCC system_header` or GNU line-marker changes inside that source. Those
source-region changes and documentation attachment are separate layers.
Canonical read paths, compiler-visible accessed spelling, and diagnostic `#line`
names remain separate; see [Header paths](header-paths.md).

## Bounds and validation

The combined configured root count is limited to 65,536 before resolution or
index allocation. Zero or one root uses direct lookup without allocating a
search index, as does filesystem-disabled preprocessing. The optional provenance
catalog remains opt-in; the system flag adds no standalone allocation.

GCC 13 and Clang 18 probes cover regular/system duplicates, symlink aliases,
include-next chains, configured spelling, and compiler identity after macro
undefinition. Regression tests also cover per-run symlink changes, inherited
system class, file allowlisting, and root-count limits. Exact commands and results
are retained with the include-search evidence artifact.

The saved Linux x86-64 measurement compares the exact prior composition with
this layer. Default Builder and facade cases with 1, 100, and 1,000 declarations
have equal allocation counts and bytes. Nine real header routes, each with file
origins disabled and enabled, preserve source and semantic outputs; all eight
requested Rust binding outputs are byte-equal. Multi-root runs add three search
allocations and 232–240 allocated bytes without file origins. With origins,
allocation calls stay at the same three-entry increase and bytes rise by
264–2,256, including the wider optional mapping rows.

| In-memory type | Before | After |
| --- | ---: | ---: |
| Preprocessor configuration | 192 | 216 |
| Stateful preprocessor | 384 | 416 |
| Optional file mapping row | 48 | 56 |
| Semantic declaration | 136 | 136 |

A repeated-root stress probe from 10 to 10,000 roots shows linear filesystem
resolution work, with three additional allocations; zero/one-root preprocessing
has identical allocations. Timings were collected on a shared host and are not
release performance guarantees. The artifact is
[`include-search-2026-09-08.json.gz`](../corpus/evidence/include-search-2026-09-08.json.gz).
