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

## Hard links

On Unix hosts, headers with multiple hard links share `#pragma once` state and
Clang's first registered name. The key is the device and inode obtained from the
same open file used to read the source. The index is populated only when the link
count exceeds one and resets with each translation unit. Ordinary files continue
to use the canonical-path index. Input files must remain stable during a
preprocessing operation; cached file identities are not persisted across calls.

Canonical read paths remain separate from that shared identity. GNU uses each
access spelling and quoted-include directory, while Clang uses the first
registered name and its directory. Each inclusion retains its own search origin
for `#include_next`. The main file is opened and registered before forced inputs;
its contents are still processed last, including after an earlier once marker.

The dependency list retains distinct canonical hard-link paths. When once skips
a hard-link alias, Clang records that path and GNU omits it. Physical origin
entries still identify the canonical path actually read; their accessed-name
entries retain the compiler-visible spelling. Logical `#line` names remain
separate from both.

Native dependency comparisons use `-E -MD` so expansion and dependency collection
run together. GNU `-M` alone omits normal-text macro expansion: a written
`_Pragma("once")` therefore affects the first mode but not the second. The saved
probes preserve this difference rather than treating a dependency-only run as an
equivalent preprocessing oracle.

### Windows boundary

Ordinary Windows headers with one link remain supported. Headers reporting
multiple or unknown links produce an explicit identity diagnostic. The safe
`winapi-util` query provides the link count, but its 64-bit file index is not used
for coalescing. Microsoft documents that this index is not unique on ReFS, which
supports hard links; a supported safe interface to the open handle's 128-bit ID
is still required. See [file information](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information)
and [ReFS features](https://learn.microsoft.com/en-us/windows-server/storage/refs/refs-overview).
Other hosts retain canonical-path behavior; Unix hard-link equivalence is not
claimed for them.

### Validation and cost

The [hard-link observations](../corpus/evidence/hardlink-identity-2026-09-08.json.gz)
contain 18 matching GCC 13.3/Clang 18.1.3 preprocessing and dependency pairs,
including forced-header order, guards, `_Pragma`, recursive main aliases, and
`#include_next`. All 80 preprocessor tests pass, including the native oracle hooks and existing
symlink, non-UTF-8, logical-name, and path-spelling checks. A Windows cross-check verifies the safe helper's types; this layer
does not claim a native Windows or macOS run.

All 97 semantic and scaling allocation controls are unchanged. Thirty-six runs
over nine unchanged real-header routes, with origins both enabled and disabled,
have identical allocation counts and bytes, preprocessed source, semantic dumps,
and available generated Rust files. These include crypto-only AWS-LC's 4,455
declarations; they do not constitute a generated AWS-LC consumer build.

Tracing those nine routes records no additional file opens. There is one new
handle-metadata query per opened header: 2–421 on these routes. The net `statx`
increase can be one lower because an existing capability probe is avoided when
the first metadata query succeeds. The hard-link index adds no allocations for
ordinary files. Elapsed samples are retained without a speedup claim.
