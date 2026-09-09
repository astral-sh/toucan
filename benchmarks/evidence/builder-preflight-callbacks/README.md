# Builder preflight at 7db2b850

The final untimed preflight covers the nullable function-typedef projection,
alias-cycle checks, aggregate work budget, and schema-3 analyzer. Both generators
use untouched zlib 1.3.1, SQLite 3.45.1, zstd 1.5.7, and libgit2 1.9.1 headers:
73 physical public-header roots, identical input policies, comments/default
derives, and Rust 1.64 output.

| Project | Function exports | Global exports | Matching constants | Structural API equality |
| --- | ---: | ---: | ---: | --- |
| zlib | 81 | 0 | 39 | Pass |
| SQLite | 291 | 3 | 471 | Existing returned-callback difference |
| zstd | 68 | 0 | 110 | Pass |
| libgit2 | 848 | 0 | 673 | Ten extra integer typedefs |

All 1,288 function groups and three global groups contain one public Rust export
each. All 115 complete Rust record layouts and 667 field offsets match. Sixteen
current-Rust/Rust-1.64 module compilations, eight current-Rust FFI executions, and
16 GCC/Clang probe executions pass. Independent C probes cover 17 selected records
and 88 offsets per generator/compiler, plus 1,293 emitted constants. No selected
declarations are skipped; unsupported analyzer inventories are empty.

SQLite's `xDlSym` returns a zero-argument callback in Toucan; bindgen repeats the
lookup's three arguments. The existing C type oracle supports Toucan's signature.
Libgit2 keeps ten additional integer typedefs. Both generators retain three i32
sentinels whose negative values differ from the native unsigned-64 C values.
Those differences and C expression size/signedness remain in the raw reports.

The configuration uses constant-style enums. Its schema-3 enum inventories are
empty; [separate native enum evidence](../../../corpus/evidence/binding-rustified-enums-schema3-2026-09-09.json.gz)
covers the pinned zstd and AWS rustified forms.

All eight generated outputs are byte-identical to the preceding
[43867b9 preflight](../builder-preflight-selection/README.md), with identical
requests and header hashes. The driver required no changes. The release benchmark
and schema-3 analyzer were built afresh from frozen `7db2b850`; all 2,389 source
files were verified against their Git objects before and after the run.

The [capture](capture.json.gz) includes all text outputs, commands, native probes,
source/input hashes, and binary paths/checksums. The [summary](summary.json) gives
counts and limitations. The original capture retains its `awaiting-review` status.
No timing occurred. Prior timing evidence remains scoped to its older source.
This run does not establish trait/derive-generated API, documentation, bitfield,
arbitrary-consumer, other-target, or ABI register-classification equality.
