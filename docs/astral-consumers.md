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
