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

The [Git-pinned application run](../corpus/evidence/astral-git-optin-2026-09-09/README.md)
builds both complete pinned applications with the new feature and compares them
with their untouched upstream defaults. The driver ran at `d00076d`; both jobs
compiled the frontend from Git revision `85bf1ad`. They passed in fresh Linux
images without libclang, using stable Rust 1.98.1:

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
`run-astral-optin` pull-request label. A label request uses the driver and integration
patches from the checked-out PR merge commit, which the artifacts record. The
frontend still comes from the separate Git pin `85bf1ad`. Repeating this fixed-pin
run cannot validate newer frontend code; that needs a separately reviewed pin and
source inventory, followed by another acceptance run. New commits do not automatically
repeat the run: dispatch again or remove and reapply the label. The two Linux
jobs share one request group, so a new explicit request cancels an older one;
unrelated label events cannot cancel it. Opening or updating a stack PR does
not allocate an application runner.

## Git-pinned trial

The trial uses the private Git dependency at
`85bf1ad6dcbc5840ade11bf8798785b6da13260a`. Repository access is required;
no crates are published by this workflow. The optional generator requires Rust
1.96 on the build host, which both pinned workspaces already use. Compatibility
with zstd's older default build toolchains remains a separate gate.
Cargo can resolve optional Git dependencies for default builds too, so the
patched workspaces require that access even without `toucan-zstd` enabled.

The new application workflow keeps that Git dependency intact. A fetch-only
step uses the same repository's read-only `GITHUB_TOKEN`; it does not compile
any dependency. Its credential helper answers only the exact Toucan HTTPS
repository and stores no credentials. The token is absent from subsequent
steps. The driver fetches remaining public inputs, then builds and tests offline.

Before compilation, the gate verifies the fetched checkout's actual Git HEAD,
the exact 630-file frontend inventory, and all nine frontend Cargo packages.
The build's compiler-artifact messages must name those same package IDs,
manifest paths and source paths. The existing zstd dep-info checks then prove
which freshly generated `OUT_DIR` bindings the applications consumed. Package
source IDs alone are insufficient: an earlier pin audit found that Cargo could
label an escaped local path with the requested Git source.

The [successful Git-mode run](https://github.com/astral-sh/toucan/actions/runs/34394853038)
passes both application gates with the Git dependency intact. Its independent
artifact audit checks all 34 command records, source inventories, generated
inputs, and the four uploaded application executables. Frontend library hashes
were recorded in CI, but their bytes were not uploaded for independent rehashing.
The earlier [local-source run at `131ec7a`](../corpus/evidence/astral-optin-2026-09-09/README.md)
remains separate evidence. Registry naming, publishing, and public distribution
are deferred.

## Subsequent integrations

AWS-LC needs its own feature in `aws-lc-sys`, forwarded through `aws-lc-rs` to uv.
Its Builder and TLS consumer paths already have [native evidence](uv-tls-consumer.md),
but their build-script integration and clean build need separate acceptance.
The current external FIPS route still calls libclang, and the external SSL route
needs an upstream build-script change; see [replacement readiness](replacement-readiness.md).

Apple Silicon and Windows each need actual application builds with the selected
feature and dependency profiles before enabling generation there. Intel macOS
validation remains explicitly opt-in. These checks can follow a Linux-only trial.
