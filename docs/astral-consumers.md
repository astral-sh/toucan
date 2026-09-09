# Ruff and uv workspace consumers

The optional [`verify_astral_consumers.py`](../scripts/verify_astral_consumers.py)
runner builds `ty` from the Ruff workspace and the `uv` CLI with their upstream
bindings, then rebuilds each with Toucan's bindings in the same `zstd-sys` crate.
The [manifest](../corpus/consumers/astral.json) pins source revisions, archive
checksums, and upstream licenses. No project source or corpus is vendored here.

## Run

Use Python 3.12 or newer, an installed Rust toolchain compatible with the pinned
workspaces, their native build dependencies, and a local libzstd shared library.
The shared library encodes a test HTTP response; the workspace binaries decode it
through their own bundled `zstd-sys`. Execution requires the generated bindings'
target to match the Rust host. The recorded run used x86-64 Linux and Rust 1.98.1.

First prepare and independently test the generated crate with the existing
consumer fixture. For example, on x86-64 Linux:

```sh
cargo build --release -p toucan_cli
python3 scripts/verify_zstd_consumer.py \
  --toucan target/release/toucan \
  --target x86_64-unknown-linux-gnu \
  --profile default \
  --cache /tmp/toucan-zstd-consumer \
  --output /tmp/toucan-zstd-evidence
python3 scripts/verify_astral_consumers.py \
  --zstd-source /tmp/toucan-zstd-consumer/zstd-sys \
  --zstd-evidence /tmp/toucan-zstd-evidence/evidence.json \
  --cache /tmp/toucan-astral-consumers \
  --output /tmp/toucan-astral-evidence
```

`--project ty` or `--project uv` selects one workspace. `--offline` requires cached
archives and Cargo dependencies. `--rust-toolchain` selects an installed toolchain;
`--python` selects an existing interpreter for the uv case. The default build
limit is 30 minutes per command, configurable with `--build-timeout`. These are
debug builds using each workspace's own profiles, not a performance benchmark.

The cache is dedicated to this runner. Existing extracted trees must match the
pinned archive, except for the path patch in `Cargo.lock`. Each upstream build
restores the archive's lock; each generated build changes only `zstd-sys`'s
registry source and checksum. Dependency versions, other sources and checksums,
and selected `zstd-sys` features must remain equal. The original project code,
C sources, crate root, and build scripts remain unchanged.

Cargo's selected artifact records identify the dep-info files checked for actual
binding inputs; the runner does not infer consumption from the presence of files
in a build cache. The prepared crate changes `bindings_zstd.rs` and
`bindings_zdict.rs`, but these pinned workspaces enable only `std` on `zstd-sys`
and compile `bindings_zstd.rs`. Experimental and threaded features have separate
coverage in the zstd consumer matrix.

## Upstream library tests

Add `--library-tests` to the full build command to run the manifest's selected
upstream library suites: `ty_vendored` with its `zstd` feature and `uv-extract`.
Each suite runs with upstream bindings and again with generated bindings. Cargo
must select the expected test executable and `zstd-sys` inputs for each build.
The runner rejects empty or filtered test runs and requires identical named
results. Ignored tests remain recorded as ignored.

These suites cover the bundled typeshed archive and uv's archive handling code.
The existing CLI runtime checks still run. This option does not run every test
in either workspace and cannot be combined with `--runtime-only`.

## Runtime checks

- **Ty:** a valid program imports `pathlib`, `datetime`, `collections.abc`, and
  `json`; an invalid program produces two specific assignment diagnostics. This
  reads standard-library types from ty's bundled typeshed archive. Exit codes,
  stdout, and stderr must match upstream exactly.
- **Uv:** a local HTTP server returns a deterministic pure-Python wheel with
  `Content-Encoding: zstd`. Separate uncached installs verify the module, data,
  and package metadata byte for byte, then import the installed module with an
  existing Python interpreter. A truncated compressed response must fail with an
  end-of-file diagnostic and leave the package uninstalled. The response bytes
  and requests are retained. uv requests `Accept-Encoding: identity`; this test
  deliberately supplies an encoded response to exercise decoding, not encoding
  negotiation. Installation timings are recorded but are not compared.

To repeat runtime checks with already-built binaries, select one project:

```sh
python3 scripts/verify_astral_consumers.py \
  --runtime-only --project uv \
  --upstream /tmp/toucan-astral-evidence/uv/bin/uv-upstream \
  --generated /tmp/toucan-astral-evidence/uv/bin/uv-generated \
  --output /tmp/toucan-astral-runtime
```

Runtime-only reports make no build-validation claim. Full reports also retain
source inventories' hashes, lock hashes, binary hashes, selected features,
active binding hashes, Cargo commands, and dep-info. Archive extraction uses
Python's data filter; any normalized symlink spelling is reported separately.

## Recorded result

The [expanded `dbda83c` run](../corpus/evidence/astral-consumers-dbda83c/summary.json)
passed both projects' locked builds and CLI scenarios with fresh bindings. It also
passed both `ty_vendored` tests with `zstd` enabled and all 19 `uv-extract` library
tests, with identical named results under upstream and generated bindings. No tests
in these selected suites were ignored. Retained dependency files identify the
binding inputs in both test builds and binary builds. This run used native
x86-64 Linux and does not cover every test in either workspace.

### Earlier result

The [2026-09-08 report](../corpus/evidence/astral-consumers-2026-09-08/report.json)
records successful upstream and generated builds and runtime comparisons for both
packages. All 11,118 Ruff archive files and 1,738 uv archive files were checked;
only `Cargo.lock` changed after extraction. uv's fixture symlink lost its trailing
slash during data-filtered extraction, with its target preserved.

The complete runner passed from its entry point with cached source archives and
Cargo dependencies. All four builds use locked dependency graphs. The
[artifact manifest](../corpus/evidence/astral-consumers-2026-09-08/artifacts.json)
records the runner hash and command and links retained build output and dependency
files. Both upstream and generated Cargo artifact records identify ty's host/build
and runtime `zstd-sys` instances and uv's runtime instance. The frontend binary
used to generate the bindings was built from Toucan commit `58e12d4`.

This is evidence for two real consumer paths on native x86-64 Linux. It does not
run either project's complete test suite, establish performance parity, validate
all workspace packages or features, or provide native macOS/Windows execution
coverage.

## Run the unchanged bindgen build script

The builder adapter has a separate gate:

```sh
python3 scripts/verify_astral_builder.py \
  --cache /tmp/toucan-project-consumers \
  --output /tmp/toucan-astral-builder-evidence \
  --offline
```

Use a fresh output directory. The cache can reuse the pinned source archives and
Cargo targets from the earlier consumer runner; `--offline` requires those
archives to be present. Cargo resolution runs offline and preserves each existing
locked package version and dependency edge. Newly required frontend dependencies
are recorded. The driver saves and restores each project's original lockfile and
checks its entire source inventory before and after the build. A later run can use
`--zstd-source /path/to/earlier-output/zstd-sys` to reuse that package identity and
compiled Cargo artifacts. Reuse checks every file against the pinned package and
the two exact manifest edits; pre-replaced binding files are rejected.

The gate changes only the scratch **zstd-sys Cargo.toml**:

1. Its `bindgen` build-dependency becomes the `toucan_bindgen` package.
2. Its `std` feature includes `bindgen`. Both pinned consumers already select
   `std`, so this activates binding generation in their actual builds.

The zstd-sys build script, C sources, and Rust sources stay byte-identical to the
pinned package. All ty and uv C/Rust sources and manifests stay unchanged. Cargo's
selected artifact records identify the binaries under test and the zstd-sys
libraries linked into the binary and library-test builds. Their dependency files
must reference Toucan-generated `OUT_DIR/bindings.rs` for the native Rust host,
with no checked-in binding input active.

The driver compares both `ty_vendored` tests, all 19 `uv-extract` tests, ty's exact
valid/invalid diagnostics, uv's zstd-compressed-wheel installation and installed
bytes, and rejection of a truncated zstd response with the expected EOF error.
The [builder evidence](../corpus/evidence/astral-builder-a7fab44/summary.json)
records the native Linux run and the source, lock, generated binding, selected
artifact, and runtime hashes. It covers these two pinned consumer paths; it does
not run either workspace's full test suite or measure application performance.

The [combined frontend rerun](../corpus/evidence/astral-builder-a3256d6/summary.json)
at `a3256d6` includes C90, Microsoft declaration attributes, allocation builtins,
and compiler feature queries. Fresh replacement packages drive both unchanged
project build scripts. Both ty vendored tests and all 19 uv extraction tests pass;
CLI diagnostics, installed wheel bytes, and truncated-frame rejection match. All
557 ty and 750 uv existing locked packages and dependency edges are preserved.
This remains evidence for the recorded native Linux paths, not their complete
workspace suites or other operating systems.

The [frozen frontend refresh](../corpus/evidence/astral-builder-acfb815/summary.json)
at `acfb815` runs the same unchanged zstd build scripts after the builder
compatibility and documentation layers. Both ty vendored tests and all 19 uv
extraction tests pass. Exact ty diagnostics, installed wheel bytes, and truncated
zstd-response rejection match. All 557 ty and 750 uv original locked packages
and dependency edges remain present, and Cargo artifacts identify the frozen
frontend used by both builds.

All 2,301 files in that source snapshot retained their original hashes. Concurrent
native checks added one identified Python bytecode file; the evidence preserves
the initial strict audit failure and records that generated file separately.
This refresh covers the recorded native Linux zstd paths. AWS-LC and TLS retain
their separately recorded consumer revisions; these runs do not refresh them or
measure application performance.

The [selection and object-type refresh](../corpus/evidence/astral-builder-00ec563/summary.json)
at `00ec563` repeats the unchanged zstd build-script route after name filters,
parameter-alias retention, object projections, and qualified-array compatibility.
Both ty vendored tests and all 19 uv extraction tests pass with upstream and
Toucan bindings. Ty diagnostics, installed wheel bytes, imports, and the recorded
truncated-response behavior match.

All 2,359 frozen frontend files retain their exact SHA256 hashes, with no added,
changed, or deleted files and no bytecode exceptions. Cargo artifacts and dep-info
identify all nine frontend crates from that snapshot for each binary and test
build. The original 557 ty and 750 uv locked packages and dependency edges are
preserved; only the explicit zstd-sys build-dependency substitution is added.
Upstream source inventories return to their entry state. This refresh covers the
recorded native Linux zstd routes; it does not refresh AWS-LC/TLS, run full consumer
workspace suites, or measure application performance.

The [published source mapping](../corpus/evidence/astral-builder-00ec563/published-source.json)
verifies that this snapshot has exactly the same Git tree as `b371cd0` in PR #293.
The stack reorder moved the two CI fixes into their originating PRs.
