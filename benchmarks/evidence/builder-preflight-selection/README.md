# Builder preflight at 43867b9

The untimed preflight completes on untouched zlib 1.3.1, SQLite 3.45.1, zstd 1.5.7
and libgit2 1.9.1 headers. Both generators use the same 73 physical public-header
roots, target, macro and enum policies, Rust 1.64 output, comments and derives.

| Project | Function exports | Global exports | Matching constants | Structural API equality |
| --- | ---: | ---: | ---: | --- |
| zlib | 81 | 0 | 39 | Pass |
| SQLite | 291 | 3 | 471 | Existing returned-callback difference |
| zstd | 68 | 0 | 110 | Pass |
| libgit2 | 848 | 0 | 673 | Ten extra integer typedefs |

Schema 2 retains every public export in each linker-symbol group. Every group
contains one export in this capture: 1,288 function groups represent 1,288 Rust
function names, and three global groups represent three Rust names.

The capture records 16 successful generated-module compilations across current
Rust and Rust 1.64, eight current-Rust FFI executions, and 16 GCC/Clang probe
executions. All 115 complete Rust record layouts and 667 field offsets match the
reference. Independent C probes cover 17 selected records and 88 field offsets
per generator/compiler, plus 1,293 emitted constants. C expression size and
signedness are retained separately; this does not establish C/Rust macro type
equality. Both generators retain three signed-i32 sentinel values that differ
from their native unsigned 64-bit C values. Those raw differences remain in the
report.

Compared with the corrected acfb815 baseline, zlib and SQLite now also emit
`va_list` and `__builtin_va_list`. All reference bytes, and Toucan's zstd/libgit2
bytes, are unchanged. SQLite's `xDlSym` returned callback has no parameters in
Toucan; bindgen repeats the lookup's three parameters. The existing C type
oracle supports Toucan's signature. Libgit2 retains ten additional integer
typedef aliases. Full structural equality remains false for those two projects.

The [capture](capture.json.gz) preserves the exact driver, all text outputs,
probe commands, schema 2 inventories, source/input hashes, full Git-tree audits
and binary paths/checksums. The [summary](summary.json) separates all results
and limitations. The original capture remains untouched with its
`awaiting-review` status. No timing, compilation or native execution occurred
during packaging.

The compared source is 43867b9. Full-tree audits also verify 00ec563 and show that
all frontend libraries, the Builder, benchmark and analyzer sources are
identical. The later CLI default and documentation/evidence changes do not
change this workload. This evidence covers one pinned Linux x86_64
configuration; it does not establish documentation text, trait, bitfield
accessor or arbitrary-application equivalence.

The [published source mapping](../../../corpus/evidence/astral-builder-00ec563/published-source.json)
links the full 00ec563 source tree to `b371cd0` in PR #293.
