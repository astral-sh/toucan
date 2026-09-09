# uv and ty with the pinned Git frontend

[GitHub run 34394853038](https://github.com/astral-sh/toucan/actions/runs/34394853038)
passed both application gates on September 9, 2026. The workflow and verification
driver ran at `d00076d620c34b42c17a474e32e41090288e4483`; the frontend dependency
came from Git revision `85bf1ad6dcbc5840ade11bf8798785b6da13260a`.

Each job built the default application and its proposed `toucan-zstd` feature in a
fresh Ubuntu 24.04 container without libclang, using stable Rust/Cargo 1.98.1 on
native Linux x86-64. The zstd patch retained its Git dependency throughout the
trial. The separate historical local-source application evidence is unchanged.

| Project | Preserved upstream inputs | Library tests per build | Application checks |
| --- | --- | ---: | --- |
| uv at `d28a3ee3` | 1,738 source paths; 750 locked packages and their dependency edges | 19 uv-extract tests | Identical four wheel files and imports after zstd-compressed HTTP installation; truncated responses fail with the same EOF error and install nothing. |
| ty at Ruff `e7adf82f` | 11,118 source paths; 557 locked packages and their dependency edges | 2 ty_vendored tests | Identical successful standard-library imports and two expected assignment diagnostics. |

The ty build exercises its build-time typeshed compressor and runtime reader.
Retained Cargo output and Rust dependency files identify Toucan-generated
`OUT_DIR/bindings.rs` in both graphs. uv has one runtime instance. Library tests
consume the same identified generated inputs.

## Git source and credentials

A separate metadata-only step fetched the private Git dependency with a scoped,
read-only GitHub token. Checkout credentials were not persisted. The credential
helper answers only the exact repository's HTTPS request and does not store the
token. Application builds run in a later step without that token; the driver
rejects an inherited fetch token and builds offline after fetching public inputs.

The hash-verified helper checked the fetched checkout's actual Git HEAD and exact
630-file source inventory. Retained Cargo metadata places all nine frontend
packages under the same fetched checkout. Each application and library-test build
reports those package IDs, manifests and source paths; the first application build
compiled all nine libraries freshly. The independent audit also checks the 630
source hashes against the pinned Git objects and the workflow's source inventory.

## Independent artifact audit

The [summary](summary.json) and per-project [uv](uv/audit.json) and
[ty](ty/audit.json) reports verify:

- Both downloaded ZIP files against GitHub's sizes and SHA-256 digests.
- All 34 application command records and their output hashes, including metadata
  and fetch ordering, offline builds, and the four retained ELF64 executable hashes.
- Empty libclang searches before and after the runs, null library lookup, and
  installed-package lists with no libclang package.
- All nine frontend library identities and source paths against the Git metadata,
  and each recorded library filename against the corresponding Cargo artifact row.
- Generated bindings and dependency-file hashes, named test results, original
  source inventories and lockfile edges, and retained runtime fixture bytes.

CI recorded hashes for the nine frontend libraries and their metadata files, but
those files were not uploaded. This audit retains and syntax-checks those hashes
and matches their filenames to Cargo output; it cannot independently recompute
library hashes. The four uploaded application executables were independently
rehashed before being omitted from this compact bundle.

The completed runner's Git checkout and source/header files are not available to
this auditor. Their before/after stability, actual checkout HEAD and generated
binding timestamps were checked by the recorded CI driver and helper. These
limits are separate from the independently checked retained bytes.

## Retained evidence

`capture.json.gz` contains raw command output, Cargo metadata and artifact JSON,
job logs, bootstrap package inventories, source reports, generated bindings,
dependency files, runtime fixtures and the exact verification source from the
tested driver commit. `manifest.json` records its checksum and the sizes and
inspected hashes of the omitted executables. The original full executables remain
in the GitHub artifacts during their retention period.

To repeat the audit, download a full job artifact and supply a Toucan checkout
containing both Git revisions and the pinned project archives (`ruff.tar.gz` and
`uv.tar.gz`):

```sh
python3 -B verify_artifacts.py \
  --project uv \
  --artifact "$TOUCAN_GIT_TRIAL_ARTIFACT" \
  --output "$TOUCAN_GIT_TRIAL_AUDIT" \
  --repository "$TOUCAN_GIT_TRIAL_REPOSITORY" \
  --project-archives "$TOUCAN_GIT_TRIAL_ARCHIVES"
```

Use `--project ty` for the other artifact. The verifier reads and hashes the
executables without running them or building either project.

## Scope

This is an authorized private Git trial of the proposed Linux x86-64 zstd feature.
The application manifests carry the reviewed feature patches, and the staged
zstd packages use a scoped Cargo override. Registry publication is outside this
trial. Ordinary private-Git testing requires repository access, Rust 1.96 or
newer, and the documented C build tools and headers.

These results cover the selected zstd paths and library suites. They do not cover
every workspace test, other targets, AWS-LC generation, or application performance.
The builds use development/test profiles with debug information disabled. The
selected default Ruff CLI dependency graph has no zstd path; the demonstrated
Ruff-workspace consumer is ty.
