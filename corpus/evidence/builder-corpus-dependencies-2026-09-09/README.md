# Builder dependency resolution and zstd: 2026-09-09

The corpus runner now resolves and fetches the adapter's dependencies before
the locked offline build phase. Previously, a fresh runner could cache the
original zstd dependencies without caching the adapter's new dependencies.
Both Linux architectures failed at this step in CI run 34289479613.

A separate empty Cargo home reproduces the boundary: original metadata succeeds,
candidate offline metadata fails, candidate online metadata succeeds, and then
locked offline candidate metadata succeeds. Complete metadata, commands, stderr,
and lockfiles are retained. The regular registry cache remains separate.

The updated runner passes all four zstd feature profiles with the macro Builder
implementation at 524b4c1 and the diagnostic test correction at 5e69bca. All 25
runtime artifact pairs match byte for byte. The zstd-sys build script and C/Rust
sources remain unchanged; only its Cargo.toml substitutes the binding generator.
The generated graph excludes bindgen, clang-sys, and libloading. Rust dependency
files prove that all four builds consume their generated bindings.

These executions use current Rust on native x86-64 Linux, with generated syntax
targeting Rust 1.64. They do not establish a current Rust 1.64 consumer run or
native macOS/Windows results. The earlier Windows diagnostic failure from CI run
34289479699 and the passing corrected local suite are retained separately.

`captures.tar.gz` includes the raw zstd outputs and artifacts, generated bindings,
dependency files, resolver captures, runner sources, and validation logs.
`manifest.json` records source hashes, archive identity, and the summarized scope.
