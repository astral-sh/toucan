# Opt-in zstd binding generation

These patches add an experimental `toucan-zstd` feature to the pinned ty and uv
workspaces. The feature generates zstd bindings with Toucan on native
`x86_64-unknown-linux-gnu`. Default builds keep their existing bindings.

This directory contains a proposed integration and a reproducible fixture.
[Clean builds of feature-enabled ty and uv](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/astral-git-optin-2026-09-09/README.md)
pass the selected application checks with the pinned Git frontend. The patches
are not an upstream zstd release.

## Patches

| Patch | Effect |
| --- | --- |
| `patches/zstd-rs.patch` | Adds an independent `toucan` feature to zstd-sys and forwards it through zstd-safe and zstd. Selects Toucan when both generator features are enabled. |
| `patches/ty.patch` | Adds the selector to ty_vendored's normal and build dependencies, then forwards it through ty_project and ty. |
| `patches/uv.patch` | Adds the selector to uv-extract and forwards it through uv. |

The zstd patch uses an immutable Git dependency at
`85bf1ad6dcbc5840ade11bf8798785b6da13260a`. The repository is private: fetching the
trial dependency requires repository access. Testing uses this Git pin and the
prepared zstd feature patches.

The published packages used to assemble the patch base are zstd 0.13.3,
zstd-safe 7.2.4, and zstd-sys 2.0.16+zstd.1.5.7. They have different upstream Git
revisions; preparation verifies each published archive checksum and uses its
original manifest. Ty uses Ruff revision `e7adf82ff005f3ab3051c363464cf65bf8a6e2f3`;
uv uses `d28a3ee3d0f7122b0da64b0226d2e173e7d23747`.

## Backend selection

| Consumer features | Active generation dependencies | Binding input |
| --- | --- | --- |
| Default high-level zstd | Neither generator | Existing pregenerated Rust |
| `toucan` | Toucan | Fresh `OUT_DIR/bindings.rs` |
| `toucan,bindgen` | Toucan, bindgen, and clang-sys | Fresh Toucan `OUT_DIR/bindings.rs` |

Cargo features are additive. Toucan takes precedence when both features are
selected, but it cannot remove the other feature's dependencies. The combined
case therefore makes no dependency-removal claim. Direct zstd-sys defaults still
select bindgen, as upstream does today; high-level zstd disables those defaults.

Ty packages typeshed with a build dependency and reads it with a runtime
dependency. The patch selects Toucan in both graphs. A feature on the ty binary's
runtime graph alone would not cover the build-time compressor.

The first integration deliberately rejects Toucan generation unless both `HOST`
and `TARGET` are `x86_64-unknown-linux-gnu`. It does not silently switch generators
on unsupported configurations. Existing default builds retain their selection.
Toucan needs Rust 1.96 on the build host, matching both pinned workspaces. The
unchanged generated-Rust target remains 1.64. Compatibility of default builds
with Cargo/Rust 1.64 has **not** been established: an optional Git dependency can
still affect package discovery and resolution before compilation.

## Reproduce the bounded checks

Use Python 3.12+, Git, Rust 1.96+, a native C toolchain, and caches containing the
pinned package/source archives. `prepare.py` does not download sources or modify
registry packages. It verifies the archives and the 630-file frontend inventory,
extracts fresh scratch copies, checks patch applicability, and records every
patch checksum.

Set `TOUCAN_OPTIN_SOURCE` to an immutable checkout or extracted source archive at
`66c87396bbe03b22085339a87b8c350c90be280b` or
`85bf1ad6dcbc5840ade11bf8798785b6da13260a`. A current checkout can differ from the
historical inventory, including in test files, and must then fail this check.
For local application reproduction, pass the same path to
`scripts/verify_astral_optin.py --toucan-source "$TOUCAN_OPTIN_SOURCE"`.
That driver's local mode defaults to its own checkout for backward compatibility;
the default is usable only while its files match the historical inventory.

Fixture manifests are stored as `Cargo.toml.in` and restored only in the scratch
tree. They must not participate in Cargo's discovery of Git dependency packages.

```sh
python3 prepare.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --crate-cache "$TOUCAN_OPTIN_CRATE_CACHE" \
  --toucan-source "$TOUCAN_OPTIN_SOURCE" \
  --ruff-archive "$TOUCAN_OPTIN_RUFF_ARCHIVE" \
  --uv-archive "$TOUCAN_OPTIN_UV_ARCHIVE"

python3 run_smoke.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --target-dir "$TOUCAN_OPTIN_TARGET" \
  --rust-toolchain ohm

python3 check_project_graphs.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --target-dir "$TOUCAN_OPTIN_TARGET" \
  --rust-toolchain ohm --offline
```

Set `CARGO_HOME` to the populated cache. For local Ohm runs, use the shared
`CARGO_BUILD_BUILD_DIR` separately from the supplied target directory. The
scripts accept another toolchain; omit `--rust-toolchain` to use Cargo's normal
selection for CI. Trust/reuse flags remain caller-controlled and are not enabled
by these scripts.

Preparation replaces the reviewed Git dependency with the explicitly supplied,
hash-verified local source **only in the scratch copy**. `preparation.json` records
that substitution. The source inventory comes from frontend revision
`66c87396bbe03b22085339a87b8c350c90be280b`; all 630 files also match the fetched
`85bf1ad` checkout. This avoids a hidden dependency on whichever checkout is
currently active.

The smoke runner needs a fresh work directory and target directory; it requires
each generated binding file to be written during the recorded build. Cargo JSON
and dep-info must identify the exact generated input and all nine frozen
frontend packages. The first three cases compare four bulk, streaming, and
dictionary output files and stdout byte for byte. A separate fixture compresses
data in build.rs and decompresses it at runtime, requiring two compiled zstd-sys
instances with freshly generated bindings.

The graph runner does not compile ty or uv. It checks both default and opt-in
selected-package trees, verifies the normal/build selector edges, and preserves
all original locked package versions and dependency edges. It also checks that
the selected default Ruff CLI graph contains no zstd, bindgen, or Toucan
dependency; this integration affects ty within the Ruff workspace. These graph checks
cannot establish whole-project Rust compilation or runtime acceptance.

## Run with the actual Git dependency

The historical local mode remains the default. To test the reviewed Git
manifest unchanged, fetch and verify the pin first, then select Git mode:

```sh
python3 -B corpus/consumers/zstd-optin/git_source.py \
  --cache "$TOUCAN_OPTIN_GIT_FETCH" \
  --output "$TOUCAN_OPTIN_GIT_REPORT" \
  --rust-toolchain ohm --gh-auto
python3 -B corpus/consumers/zstd-optin/prepare.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --crate-cache "$TOUCAN_OPTIN_CRATE_CACHE" \
  --frontend-mode git --git-source-report "$TOUCAN_OPTIN_GIT_REPORT"
python3 -B corpus/consumers/zstd-optin/run_smoke.py \
  --work-dir "$TOUCAN_OPTIN_WORK" \
  --target-dir "$TOUCAN_OPTIN_TARGET" --rust-toolchain ohm
```

Use fresh work, fetch and target directories. The devbox `--gh-auto` option
uses its verified OSS account and an HTTPS helper restricted to this repository;
it does not change global credentials. CI omits that option and supplies its
read-only repository token only to the fetch step. Neither route writes the
token into a URL, a Git configuration file, or the evidence. Fetching runs Cargo
metadata without compiling dependencies. Normal CI uses the repository's normal
Rust toolchain, without Ohm's local trust settings.

The verifier requires all nine packages to resolve inside one actual Git
checkout at the full revision, verifies the complete source inventory, and
matches all nine compiled library artifacts to the audited Cargo metadata.
The target directory contains build outputs; source and manifest paths must
remain inside the fetched checkout. A mixed checkout, incorrect revision,
extra or modified source file, or unaudited frontend artifact fails the gate.
The smoke runner uses offline Cargo commands, so its fixture dependencies must
already be fetched. `verify_astral_optin.py --frontend-mode git
--git-source-report REPORT` performs the corresponding public-input fetches and
offline actual application builds in the clean Linux workflow.

## Recorded results

The [actual Git-pinned smoke](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/zstd-optin-git-2026-09-09.json)
also passes all four cases, with all nine frontend libraries traced to the
verified checkout. The separate [Git-mode application gate](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/astral-git-optin-2026-09-09/README.md)
also passes complete uv and ty builds, selected library tests, and runtime
comparisons in fresh Linux images without libclang.

The earlier bounded smoke passed all four cases. Both actual workspace graphs resolve
with Toucan and no active bindgen/clang-sys dependency; their default graphs
select neither generator. All 557 Ruff and 750 uv original locked packages and
dependency edges remain present.

The initial Git pin exposed an archived Cargo.toml with an absolute path into a
live checkout. Cargo labeled those live packages with the requested Git source
ID. The packaging fix preserves that historical file as Cargo.toml.snapshot.
The corrected pin audit requires all nine frontend crates to resolve under one
fetched checkout, checks its actual Git HEAD, and verifies their source hashes.
The earlier failed audit is retained alongside the successful one.

The earlier [local-source application gate](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/astral-optin-2026-09-09/README.md)
builds actual ty and uv binaries in fresh Linux images without libclang. It
audits consumed bindings in both ty graphs and passes the selected library and
application comparisons against untouched upstream defaults. This gate uses
the hash-verified local frontend source; the original smoke used a machine with
libclang installed. See the [rollout notes](../../../docs/opt-in-rollout.md) for
the Git trial and subsequent integrations. AWS-LC/TLS
generation remains a separate feature and acceptance gate.
