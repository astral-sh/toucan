# ty and uv with Toucan zstd generation

[GitHub run 34389234082](https://github.com/astral-sh/toucan/actions/runs/34389234082)
passed both application gates at Toucan revision
`131ec7a3a5aaa9b14a68b1a5d64b667faa889a89`. Each job built the default application
and its proposed `toucan-zstd` feature in a fresh Ubuntu 24.04 container without
libclang. These are native Linux x86-64 builds using stable Rust/Cargo 1.98.1.

| Project | Preserved upstream inputs | Library tests, per build | Application checks |
| --- | --- | ---: | --- |
| uv at `d28a3ee3` | 1,738 source paths; 750 locked packages and their dependency edges | 19 uv-extract tests | Identical four wheel payload/metadata files and imports after zstd-compressed HTTP installation; truncated responses fail with the same EOF error and install nothing. |
| ty at Ruff `e7adf82f` | 11,118 source paths; 557 locked packages and their dependency edges | 2 ty_vendored tests | Identical successful standard-library imports and two expected assignment diagnostics. |

Both ty builds exercise its build-time typeshed compressor and runtime reader.
Cargo artifacts and Rust dep-info identify fresh Toucan `OUT_DIR/bindings.rs`
inputs in both dependency graphs. uv has one corresponding runtime instance.
The library-test builds consume the same identified generated inputs.

## Independent artifact audit

The [summary](summary.json) and per-project [uv](uv/audit.json) and
[ty](ty/audit.json) audits verify:

- The recorded checkout, exact driver hash, all 28 command/output records, and
  the four downloaded ELF64 executable hashes.
- Empty libclang searches before and after each run, null library lookup, and
  the retained installed-package lists containing no libclang package.
- All nine Toucan library artifacts originate in the tested checkout. The 630
  workspace/core source hashes match that Git revision and the preparation
  inventory. Both selected Cargo trees exclude bindgen and clang-sys.
- Every retained zstd dep-info and generated-binding hash, the named test results,
  the source-archive inventories, original lockfile identities and edges, and
  retained application fixture bytes.

The hash-verified CI driver also checks source stability after builds and the
fresh generation timestamps. The independent audit reads the retained artifacts;
it does not revisit the completed container's filesystem. A single extracted
uv fixture symlink is normalized from `../../../python/` to `../../../python`,
with its destination preserved and the normalization recorded.

## Retained evidence

`capture.json.gz` contains the raw command output, Cargo JSON, job logs,
bootstrap package inventories, generated bindings, dep-info, runtime fixtures,
and exact verification source from the tested revision. `manifest.json` records
its checksum and all omitted executable sizes and inspected hashes. The four
large executable files remain in the GitHub artifacts during their retention
period; this compact bundle omits them.

To repeat the independent audit against a downloaded full artifact, provide the
Toucan checkout containing the tested Git object and the pinned Ruff/uv source
archives (`ruff.tar.gz` and `uv.tar.gz`):

```sh
python3 -B verify_artifacts.py \
  --project uv \
  --artifact "$TOUCAN_OPTIN_DOWNLOADED_ARTIFACT" \
  --output "$TOUCAN_OPTIN_AUDIT_OUTPUT" \
  --repository "$TOUCAN_OPTIN_REPOSITORY" \
  --project-archives "$TOUCAN_OPTIN_PROJECT_ARCHIVES"
```

Use `--project ty` for the other artifact. The verifier reads and hashes the
executables; it does not run them or build either application.

## Scope

The application manifests carry the reviewed opt-in feature patches. The tested
zstd-sys package is supplied through a scoped Cargo override, and its optional
Toucan dependency uses the hash-verified checkout. An upstream integration or
accessible release is still needed for ordinary installation; the pinned Toucan
repository is private.

These results cover the selected Linux x86-64 zstd paths and library suites.
They do not cover every workspace test, other targets, AWS-LC generation, or
application performance. The builds use development/test profiles with debug
information disabled. The selected default Ruff CLI dependency graph has no zstd
path; the demonstrated Ruff-workspace consumer is ty.
