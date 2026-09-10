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
target to match the Rust host.

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

The driver compares the selected upstream library tests, ty's exact valid/invalid
diagnostics, uv's zstd-compressed-wheel installation and installed bytes, and
rejection of a truncated zstd response with the expected EOF error. It covers
these two pinned consumer paths, not either workspace's full test suite or
application performance.

For acceptance of the current Toucan checkout without libclang, use the
[opt-in application gate](opt-in-rollout.md).
[Historical captures](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence)
retain earlier source inventories, build logs, and runtime results; their
revision-specific results do not validate the current checkout.
