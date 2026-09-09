# Comment provenance: 2026-09-09

This layer retains raw comments and physical token locations during preprocessing.
It does not yet attach documentation to semantic declarations or emit Rust docs.

The frozen prerequisite passes 896 workspace tests, with 231 opt-in tests ignored.
Root integration passes the preprocessor and adapter suites, focused facade
tests, workspace/fuzz Clippy, and five campaign-runner tests. All 18 saved
native-backed macro-comment provenance comparisons retain their exact output.
Their source inputs and bindgen 0.72.1/libclang 18 reference output are archived.

Twenty-seven runs cover nine real-header routes with capture disabled and enabled.
Preprocessed text, semantic output, and the eight requested binding files match
the corresponding baseline. Disabled capture has unchanged allocation counts
and bytes. Enabled capture on inputs without matching comments also has zero
allocation deltas, including both zlib routes and 1/100/1,000 declaration controls.
Enabled cumulative allocation deltas are recorded separately: approximately
1.5 MB for SQLite, 0.75 MB for zstd, 24.8 MB for libgit2, and 20.0 MB for AWS-LC.
These are not peak-memory or release-speed measurements.

The saved sanitizer log completes 477,415 preprocessing executions in 181 seconds,
with 514 MiB peak RSS and an empty artifact directory. Its exact source snapshot,
binary identity, starting corpus, and log are retained. The original process exit
code was not separately captured, so this is a saved log observation, not a newly
verified root sanitizer campaign. The snapshot's Rust sources match the frozen
prerequisite. Root integration additionally preserves the macro Builder and nested
enum layers and has separate source hashes.

`captures.tar.gz` contains prototype drivers, results and binding outputs, root
validation, the frozen patch, and the saved sanitizer records. `manifest.json`
records hashes and precise test totals. Machine-local paths in captured commands
need adjustment for replay. There is no new native macOS/Windows execution or
generated AWS-LC consumer claim in this layer.
