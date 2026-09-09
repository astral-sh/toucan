# Native Linux zstd opt-in smoke

The staged zstd feature passed its bounded fixture on
`x86_64-unknown-linux-gnu`, using the frozen Toucan frontend at `66c87396` and
Ohm for local builds. This is feature-selection and functionality evidence;
there is no application-performance claim.

| Case | zstd-sys instances | Selected bindings | Result |
| --- | ---: | --- | --- |
| Default high-level zstd | 1 | Pregenerated | Passed |
| Toucan only | 1 | Fresh Toucan output | Passed; bindgen/clang-sys absent from active graph |
| Toucan and bindgen | 1 | Fresh Toucan output | Passed; both dependencies remain in the additive graph |
| Build-time compressor and runtime decoder | 2 | Fresh Toucan output in both graphs | Passed |

The first three cases have identical stdout and four identical bulk, streaming,
and dictionary output files. The fourth is a separate fixture exercising the
host/build and target/runtime dependency kinds needed by ty_vendored. Cargo JSON
and Rust dep-info identify every binding input. All generated files were written
during their recorded builds. `cargo_artifact_reused: false` means Cargo compiled
the selected library rather than reporting a cached artifact.

The [smoke summary](summary.json) records the selected artifacts and hashes.
The [workspace graph report](project-graphs.json) resolves the proposed manifests
against actual pinned ty and uv source: both opt-in trees select Toucan without
bindgen/clang-sys, and defaults select neither generator. All 557 Ruff and 750 uv
original locked package versions and dependency edges remain present. The
[Ruff CLI baseline](ruff-baseline-graph.json) has no selected zstd, bindgen,
clang-sys, or Toucan package on this target.

## Git dependency audit

The [initial audit](git-pin-audit.json) found that Cargo's discovery of an archived
manifest could select live-checkout packages while labeling them with Git source
revision `66c87396`. This was detected during metadata inspection before any
Git-bound compilation. Local-source smoke builds used the separately frozen,
hash-verified source.

The [corrected audit](git-pin-85bf1ad-audit.json) verifies the packaging fix at
`85bf1ad6dcbc5840ade11bf8798785b6da13260a`: all nine frontend packages resolve
under one fetched Git checkout with that actual HEAD. All 630 workspace/core
source files match the frozen frontend. The archived manifest retains its
original bytes under `Cargo.toml.snapshot`.

The repository is private, so this fetch used repository authorization. The pin
is an authenticated trial dependency, not a public installation route.

## Scope

No feature-enabled full ty/uv build ran here, and libclang was installed on the
smoke host. The next gate must build and exercise the actual projects in a clean
Linux image without libclang. AWS-LC/TLS adoption is separate. Old Cargo/Rust 1.64
default-path compatibility was not tested; the optional frontend uses Rust 1.96.

[manifest.json](manifest.json) records package-file and capture hashes.
[capture.json.gz](capture.json.gz) contains the recorded commands, Cargo output,
dep-info, generated bindings, runtime artifacts, graph output, and both Git pin
audits. The portable scripts were subsequently checked on a fresh preparation
and graph-only reproduction; the raw smoke driver is retained with the capture.
