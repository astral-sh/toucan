# Header access paths

The preprocessor now separates canonical filesystem identity from compiler-visible
input names. Quoted includes beside symlinks use the accessed directory; GNU uses
each spelling, while Clang retains the first registered name. Literal dot/parent
components, diagnostic `#line`, forced-header order, and the main-file `once`
difference are covered by native GCC 13.3/Clang 18.1.3 probes. The new ordered
`preprocess_files` / `parse_files` APIs preserve the last header as the main file.

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

### Validation

Native preprocessor tests compare GCC and Clang expansion and dependency output
for symlinks, hard links, forced-header order, guards, `_Pragma`, recursive main
aliases, and `#include_next`. Each opened header requires a handle-metadata query;
ordinary single-link files do not allocate a hard-link index.

Historical [access-path observations](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/accessed-header-paths-2026-09-08.json.gz)
and [hard-link observations](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/hardlink-identity-2026-09-08.json.gz)
retain the original commands and measurements for their recorded revisions.
