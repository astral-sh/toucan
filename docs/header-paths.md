# Header access paths

The preprocessor now separates canonical filesystem identity from compiler-visible
input names. Quoted includes beside symlinks use the accessed directory; GNU uses
each spelling, while Clang retains the first registered name. Literal dot/parent
components, diagnostic `#line`, forced-header order, and the main-file `once`
difference are covered by native GCC 13.3/Clang 18.1.3 probes. The new ordered
`preprocess_files` / `parse_files` APIs preserve the last header as the main file.

The frozen prerequisite's preprocessor suite passes 77 tests, including native oracles.
Thirty-six additional saved native commands match (34 accepted outputs and two
missing-main-file failures). That workspace passes 820 tests, with 216 opt-in tests
ignored; workspace Clippy and Rustdoc pass. Nine unchanged real header routes
(GNU/Clang zlib, SQLite, zstd, libgit2, plus crypto-only AWS-LC) retain identical
preprocessing and semantic output, and all eight previously generated Rust
artifacts remain byte-identical. AWS-LC still has 4,455 declarations and 5,055
opt-in origin entries.

Default allocation calls remain unchanged for those routes. The dependency index
adds 176–4,834 bytes per complete parse; all 88 profile/mode semantic cases and
1/100/1,000-declaration in-memory preprocessing/facade checks have zero allocation
or byte deltas. A first noncanonical Clang file name may require additional path
storage; the origin catalog remains opt-in. Alternating shared-host timing samples
are retained without a speedup claim. The initial concurrent artifact failure and
successful sequential rerun are both recorded in the evidence.

Evidence: [accessed header paths](../corpus/evidence/accessed-header-paths-2026-09-08.json.gz).

The [stack integration](../corpus/evidence/accessed-header-paths-root-integration-2026-09-08.json.gz)
on top of `493a6fa` passes 838 workspace tests, six native path tests, and workspace
and fuzz Clippy. Eight header outputs and four musl outputs remain byte-identical.
The fuzz target also checks exact access-name lookup at both ends of each origin
range. This integration adds no timing or sanitizer-run claim.

Filesystem identity currently uses canonical paths. Distinct hard links to one
inode are not yet coalesced for `#pragma once` or Clang's first-name lookup;
that behavior needs a separate host file-identity implementation.
