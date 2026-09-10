# Opt-in integration in uv and ty

The [prepared patches](../corpus/consumers/zstd-optin/README.md) add an experimental
`toucan-zstd` Cargo feature to pinned uv and ty sources on native
`x86_64-unknown-linux-gnu`. Upstream releases do not provide this feature yet.
Toucan generates Rust bindings during the build; the existing C compiler builds
zstd. Toucan is not linked into the shipped application.

```console
cargo build -p uv --features toucan-zstd
cargo build -p ty --features toucan-zstd
```

| Layer | Change |
| --- | --- |
| `zstd-sys` | Optional `toucan_bindgen` build dependency generates bindings into `OUT_DIR`; the crate includes that file. |
| `zstd-safe` and `zstd` | Forward the backend feature without changing wrapper APIs. |
| uv | Forward `toucan-zstd` through `uv-extract`. |
| ty | Forward through `ty_project` and `ty_vendored`, including both normal and build dependencies. |

The pinned defaults use pregenerated zstd bindings. Cargo features are additive:
Toucan takes precedence if both generators are enabled, but bindgen remains in
that combined dependency graph. The Toucan-only path excludes bindgen and
clang-sys. Unsupported host/target pairs receive an explicit error. The pinned
Ruff CLI's native Linux normal/build graph contains no zstd or bindgen; ty is
the affected consumer in that repository.

## Application acceptance

The [Linux workflow](../.github/workflows/astral-optin.yml) runs
[verify_astral_optin.py](../scripts/verify_astral_optin.py) in a pinned Ubuntu
container with a native C toolchain and no libclang. It builds untouched upstream
defaults and patched feature-enabled applications, then compares selected tests
and runtime behavior:

| Application | Generated input required | Comparisons |
| --- | --- | --- |
| uv | Fresh zstd-sys bindings | Extraction tests, wheel contents and imports, truncated-response rejection |
| ty | Fresh zstd-sys bindings in both build and runtime instances | Vendored-library tests and valid/invalid Python diagnostics |

The maintained gate tests **the checked-out frontend**, including a PR merge
commit. Local runs default to the driver's checkout; `--toucan-source PATH`
selects another Git checkout and accepts its local edits. Preparation records
HEAD, Git status and hashes of Cargo manifests, the lockfile and all crate files.
Those inputs must remain unchanged throughout the run. Cargo metadata and
compiled library artifacts must resolve to that selected source; generated
binding files must be fresh and appear in the consuming Rust compiler's dep-info.
Pinned application and zstd archives, patch checksums, lockfile preservation,
feature selection and runtime comparisons remain part of the gate.

Request a run with `workflow_dispatch` on the intended branch or the
`run-astral-optin` pull-request label. New commits do not repeat it automatically:
dispatch again or remove and reapply the label. A new request cancels an older
request. Opening a PR alone does not allocate these
application runners. Reports identify both the driver commit and frontend inputs.

These are complete application builds with selected tests, not complete upstream
workspace suites. They do not measure application speed or establish support for
other targets. Each frontend or integration change needs a new acceptance run.

## Historical replay

The original trial used frontend revision
`85bf1ad6dcbc5840ade11bf8798785b6da13260a`. Its frozen inventory remains available
only for explicit `--frontend-mode historical-git` reproduction; see the
[replay commands](../corpus/consumers/zstd-optin/README.md#historical-replay).
[Archived results](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/astral-git-optin-2026-09-09)
describe that revision, not the current frontend. The maintained local mode
substitutes the selected checkout into a scratch manifest and needs no separate
frontend Git fetch.

## Subsequent integrations

AWS-LC needs its own feature and application acceptance; see the
[TLS consumer](uv-tls-consumer.md) and
[replacement scope](replacement-readiness.md). Apple Silicon and Windows need
application builds with their selected features and dependency profiles.
Compatibility with zstd's older default build toolchains also needs a separate
gate. Registry naming, publishing and upstream adoption remain open work.
