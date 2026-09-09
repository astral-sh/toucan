# Opt-in integration in uv and ty

The first proposed integration is a build-time Cargo feature for native
`x86_64-unknown-linux-gnu`. Toucan generates the Rust interface to a C dependency;
the existing C compiler still builds that library. Toucan is a build dependency
and is not linked into the shipped uv or ty executable.

The [prepared patches](../corpus/consumers/zstd-optin/README.md) add these commands
to the pinned source trees. Upstream releases do not yet provide the features:

```console
cargo build -p uv --features toucan-zstd
cargo build -p ty --features toucan-zstd
```

## Where the changes live

| Layer | Change |
| --- | --- |
| `zstd-sys` | Add an optional `toucan_bindgen` build dependency and `toucan` feature. The build script selects its Builder and writes generated Rust to `OUT_DIR`; the crate includes that file. |
| `zstd-safe` and `zstd` | Forward the backend feature for direct library consumers. Existing wrapper APIs remain the same. |
| uv | Forward `toucan-zstd` through `uv-extract`, which selects the shared `zstd-sys` backend. Archive handling continues to use the existing compression wrappers. |
| ty | Forward the feature through `ty_project` and `ty_vendored`. Select it in both normal and build dependencies, covering typeshed compression during the build and decompression at runtime. |

The pinned uv and ty defaults already use pregenerated zstd bindings. Their new
feature would request fresh Toucan generation. It is not a runtime CLI option,
and the generation benchmarks do not measure uv or ty execution speed.

Cargo features are additive. Toucan takes precedence when both generator
features are enabled, but the bindgen dependency remains in that combined graph.
The Toucan-only path excludes bindgen and clang-sys. Unsupported host/target
pairs receive an explicit error in this first integration.

Ruff and ty share a repository, but the pinned Ruff CLI's native Linux x86-64
normal/build dependency graph contains neither zstd nor bindgen. This first
integration therefore needs no Ruff CLI selector; the relevant consumer in that
repository is ty. Other Ruff targets and dependency profiles need their own audit.

## Application acceptance

The earlier [paired uv and ty builds](astral-consumers.md#run-the-unchanged-bindgen-build-script)
prove the selected zstd consumer paths using a manifest substitution. The new
[opt-in evidence](../corpus/evidence/zstd-optin-2026-09-09/README.md) covers actual
feature selection, fresh generated binding inputs, matching compression and
dictionary artifacts, and a build/runtime dependency fixture.

The [clean application run at `131ec7a`](../corpus/evidence/astral-optin-2026-09-09/README.md)
builds both complete pinned applications with the new feature and compares them
with their untouched upstream defaults. Both jobs passed in fresh Linux images
without libclang, using stable Rust 1.98.1:

| Application | Consumed Toucan output | Matching checks |
| --- | --- | --- |
| uv | Fresh zstd-sys bindings | 19 extraction tests, installed wheel contents and imports, and rejection of a truncated response |
| ty | Fresh zstd-sys bindings in both build and runtime instances | Two vendored-library tests and valid/invalid Python diagnostics |

The retained evidence identifies the original application revisions, source
inventories, lockfile changes, Cargo features, compiler artifacts, and consumed
binding files. These are full application builds with selected tests and runtime
checks; the complete upstream workspace suites were not run.

The explicitly requested [Linux acceptance workflow](../.github/workflows/astral-optin.yml)
runs [verify_astral_optin.py](../scripts/verify_astral_optin.py) in a pinned Ubuntu
container with a native C toolchain and no libclang. It compares the untouched
upstream default build with the patched feature build, verifies source and lock
inventories, and audits both ty dependency instances. The gate has one bounded
job per application and no macOS runner. The successful run validates the
prepared patches against that exact source. Changes to the frontend, patches,
application revisions, or selected profiles need another acceptance run.

Request it with `workflow_dispatch` on the intended branch or by applying the
`run-astral-optin` pull-request label. A label request validates the checked-out
PR merge commit, which the artifacts record. New commits do not automatically
repeat the run: dispatch again or remove and reapply the label. The two Linux
jobs share one request group, so a new explicit request cancels an older one;
unrelated label events cannot cancel it. Opening or updating a stack PR does
not allocate an application runner.

## Landing the opt-in

The trial pins Toucan through Git. The repository is private, so this requires
repository access. A public integration needs published Toucan crates or a
public dependency source before it can land; an optional private Git dependency
can still prevent Cargo from resolving a default build. The opt-in generator
requires Rust 1.96 on the build host, which both pinned workspaces already use.
Compatibility with zstd's older default build toolchains remains a separate gate.

The clean application gate substitutes a hash-verified local Toucan source for
the private Git dependency. The separate Git audit verifies package discovery
and source identity at the trial pin; it does not establish public installation.
Before landing upstream, replace that dependency with the intended accessible
release, validate zstd's default toolchains, and run the corresponding project
CI with the final manifests and lockfiles. The Linux application gate above has
passed; dependency distribution and upstream integration remain open.

## Subsequent integrations

AWS-LC needs its own feature in `aws-lc-sys`, forwarded through `aws-lc-rs` to uv.
Its Builder and TLS consumer paths already have [native evidence](uv-tls-consumer.md),
but their build-script integration and clean build need separate acceptance.
The current external FIPS route still calls libclang, and the external SSL route
needs an upstream build-script change; see [replacement readiness](replacement-readiness.md).

Apple Silicon and Windows each need actual application builds with the selected
feature and dependency profiles before enabling generation there. Intel macOS
validation remains explicitly opt-in. These checks can follow a Linux-only trial.
