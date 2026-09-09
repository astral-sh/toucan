# Integrated declaration origins

The combined non-object-query, omitted-conditional, floating-name and declaration-origin implementation passes 831 workspace tests; 222 native/tool-dependent tests remain opt-in. Workspace and fuzz Clippy and Rustdoc pass.

Both ordinary/retained and ordinary/origin-capture replays agree on all 8,712 seed/profile/mode pairs, validating 37,007 origins. A further combined graph-and-origin replay passes the same pairs; 320 preprocessing inputs check physical ranges and macro locations. The fuzz targets now retain and validate these catalogs.

Eight real header binding outputs and four musl outputs remain byte-identical. Seven real translation units under GNU and Clang, through Toucan and native preprocessing, yield 28 equal ordinary/retained pairs (56 analyses). Retained analyses also capture declaration and file origins. All routes explicitly use GNU11 and the documented enlarged limits.

`summary.json` records exact changed sources, parent, binary hashes and limitations. The deterministic archive contains logs, reports, drivers and input hashes; large raw preprocessed files and executable binaries remain in the named external caches. The frozen prerequisite has separate AWS-LC header and allocation evidence. Native ARM fixture linking and sanitizer campaigns follow in separate layers.
