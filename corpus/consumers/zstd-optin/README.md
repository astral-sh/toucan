# Opt-in zstd binding generation

These experimental patches add a `toucan-zstd` feature to pinned ty and uv
workspaces on native `x86_64-unknown-linux-gnu`. Default builds keep their existing
bindings. This fixture is a proposed integration, not an upstream release.

| Patch | Effect |
| --- | --- |
| `patches/zstd-rs.patch` | Adds `toucan` to zstd-sys and forwards it through zstd-safe and zstd. Toucan takes precedence when both generators are enabled. |
| `patches/ty.patch` | Selects Toucan in ty_vendored's normal and build dependencies, then forwards through ty_project and ty. |
| `patches/uv.patch` | Forwards the selector through uv-extract and uv. |

Preparation verifies published archives for zstd 0.13.3, zstd-safe 7.2.4 and
zstd-sys 2.0.16+zstd.1.5.7. Application sources remain pinned to Ruff
`e7adf82ff005f3ab3051c363464cf65bf8a6e2f3` (ty) and uv
`d28a3ee3d0f7122b0da64b0226d2e173e7d23747`. It applies checksummed patches only
in fresh scratch directories. Fixture `Cargo.toml.in` files are restored there,
so they do not participate in discovery of frontend Git packages.

## Validate the current frontend

Use Python 3.12+, Git, Rust 1.96+, a native C toolchain and a populated Cargo
cache. Set `TOUCAN_OPTIN_SOURCE` to the checkout under test; local edits are
accepted and recorded. `prepare.py` snapshots its HEAD, Git status, Cargo
manifests/lockfile and all crate files. Source changes during validation fail.
The patch's historical Git dependency is replaced with this path in the scratch
manifest, and `preparation.json` records the substitution and source hashes.

```sh
python3 corpus/consumers/zstd-optin/prepare.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --crate-cache "$TOUCAN_OPTIN_CRATE_CACHE" \
  --toucan-source "$TOUCAN_OPTIN_SOURCE" \
  --ruff-archive "$TOUCAN_OPTIN_RUFF_ARCHIVE" \
  --uv-archive "$TOUCAN_OPTIN_UV_ARCHIVE"

python3 corpus/consumers/zstd-optin/run_smoke.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --target-dir "$TOUCAN_OPTIN_TARGET" --rust-toolchain ohm

python3 corpus/consumers/zstd-optin/check_project_graphs.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --target-dir "$TOUCAN_OPTIN_TARGET" --rust-toolchain ohm --offline
```

The archive arguments are optional for the four-case zstd smoke. Set `CARGO_HOME`
to the populated cache. Use fresh work and target directories. Local Ohm runs
use a separate shared `CARGO_BUILD_BUILD_DIR`; omit `--rust-toolchain` for CI's
normal Cargo selection. The scripts do not enable local trust/reuse flags.

The smoke checks default, Toucan-only, combined-generator and build/runtime
profiles. Cargo metadata and library artifacts must identify every frontend
package reachable from the adapter's manifests inside the selected checkout.
Escaped manifests/targets, missing packages and unaudited artifacts fail. Each
generated binding must be written during the build and consumed through dep-info.
Bulk, streaming and dictionary outputs are compared byte for byte; a separate
fixture compresses in build.rs and decompresses at runtime.

The graph runner checks both default and opt-in application dependency trees,
normal/build selector edges and preservation of original locked package versions
and edges. It checks that the selected default Ruff CLI graph has no zstd,
bindgen or Toucan dependency. These graph checks do not compile the applications.

For complete application builds and selected runtime tests, use
`scripts/verify_astral_optin.py --project uv` (or `ty`) with fresh `--cache` and
`--output` paths in a libclang-free Linux environment. It defaults to its own
checkout; `--toucan-source PATH` selects another. `--prepare-only` checks source
preparation without building applications. See the
[acceptance workflow contract](../../../docs/opt-in-rollout.md#application-acceptance).
Reports and build artifacts belong in ignored local output or CI uploads.

## Historical replay

The patch retains the original Git dependency at
`85bf1ad6dcbc5840ade11bf8798785b6da13260a` for explicit replay. Only this mode uses
`source-digests.json` and its historical package set. It requires repository
access and rejects different revisions or source inventories.

```sh
python3 -B corpus/consumers/zstd-optin/git_source.py --historical \
  --cache "$TOUCAN_OPTIN_GIT_FETCH" \
  --output "$TOUCAN_OPTIN_GIT_REPORT" --rust-toolchain ohm --gh-auto
python3 -B corpus/consumers/zstd-optin/prepare.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --crate-cache "$TOUCAN_OPTIN_CRATE_CACHE" \
  --frontend-mode historical-git --git-source-report "$TOUCAN_OPTIN_GIT_REPORT"
python3 -B corpus/consumers/zstd-optin/run_smoke.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --target-dir "$TOUCAN_OPTIN_TARGET" --rust-toolchain ohm
```

The fetch-only step uses repository-scoped credentials without writing tokens to
URLs, configuration files or reports. Do not pass its token to build commands.
Historical application replay accepts the same `--frontend-mode historical-git
--git-source-report REPORT` arguments. These results describe the frozen trial;
[archived captures](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/astral-git-optin-2026-09-09)
are separate from validation of current code.
