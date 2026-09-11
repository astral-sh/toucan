# uv HTTPS consumer gate

`scripts/verify_uv_tls.py` exercises the pinned uv executable through local HTTPS
wheel installation. It covers successful certificate verification and download,
installed-byte checks, Python import, and certificate rejection. Its first native
gate is x86-64 GNU/Linux.

The fixture creates a test CA, a server certificate valid only for `localhost`,
and an unrelated CA. Trust is supplied through `SSL_CERT_FILE` for each uv
process. No certificate is installed into the host trust store. The pinned
uv-client implementation uses its rustls backend with those trust roots.

For both TLS 1.2 and TLS 1.3:

- Trusting the test CA downloads and installs the deterministic wheel. Four
  installed files must match the fixture bytes, and its Python module must import.
- Supplying the unrelated CA must fail with an unknown-issuer error.
- Connecting to `127.0.0.1` with the trusted CA must fail hostname verification:
  the certificate is valid for `localhost` only.

Both rejection cases must stop before any HTTP request reaches the wheel
handler, and the fixture package must remain uninstalled. The server records
successful requests, negotiated TLS versions, and ciphers. uv runs with a fresh
installation directory, disabled cache and configuration discovery, no indexes
or dependencies, no Python downloads, and no inherited proxy or certificate
settings.

## Audit the actual executable

The audit takes the frozen executable's SHA-256 recorded when it was built,
its Cargo JSON build log, the pinned source directory, and its source archive:

```sh
python3 scripts/verify_uv_tls.py \
  --binary /path/to/frozen/uv \
  --binary-sha256 <hash-recorded-at-build-time> \
  --cargo-log /path/to/cargo-build.jsonl \
  --source /path/to/uv-d28a3ee3d0f7122b0da64b0226d2e173e7d23747 \
  --archive /path/to/uv.tar.gz \
  --bindings toucan \
  --output /path/to/fresh-evidence
```

The executable recorded in the Cargo log must still exist and have the same
SHA-256 as the supplied binary. A frozen copy is accepted when its bytes match;
an executable from another build is rejected before runtime checks.

The source archive and all 1,738 extracted paths are checked against the pinned
uv revision. The selected Cargo artifacts and their Rust dependency files must
include uv-client's actual TLS implementation and the compiled chain through
reqwest 0.13.4, rustls 0.23.43, aws-lc-rs 1.18.0, and aws-lc-sys 0.44.0. Cargo's
dependency graph must connect the uv binary to that chain. Rustls must select its
AWS-LC provider; the audit rejects its ring provider and reqwest's native TLS
features.

Both AWS-LC wrapper packages are verified against their Cargo-locked crate
archives. For generated bindings, the sys source may differ only in the two
reviewed manifest edits below. The compiled sys crate must enable `bindgen` and
consume the generated `OUT_DIR/bindings.rs` under `use_bindgen_pregenerated`.
The consumed file must carry the expected generator marker; a shipped crypto
binding file is rejected in either generated mode. `--bindings bindgen` selects
the bindgen 0.72.1 reference, while `--bindings pregenerated` explicitly audits
the upstream prebuilt-binding baseline.

The dependency audit uses a scratch copy of the source. For the prebuilt
baseline it restores the archived upstream lock in that copy. Generated builds
retain their saved resolved lock and exact local sys-package configuration. The
original source and lock are checked again afterward.

## Prepare the generated-binding builds

Use separate copies of the pinned uv archive, separate sys-package copies, and
separate Cargo target directories for reference and candidate. Keep all uv and
AWS-LC C/Rust sources and uv manifests unchanged. Resolve and preserve the
scratch uv lockfiles using the existing Astral builder driver's package and
dependency-edge checks.

Both sys-package copies change `prebuilt-nasm = []` to
`prebuilt-nasm = ["bindgen"]`. The pinned uv graph already enables `prebuilt-nasm`,
so this activates generation without changing any uv feature or dependency.
The reference keeps its bindgen dependency, locked to 0.72.1. The candidate also
replaces that optional build dependency with the local `toucan_bindgen` package.
No other sys-package bytes may change.

Pass the copied sys package through Cargo's scoped
`--config patch.crates-io.aws-lc-sys.path=...` option. Build `-p uv` with the same
native C compiler, resource headers, sysroot, CFLAGS, Rust profile, and locked
existing dependency versions on both sides. Save Cargo's JSON build output,
freeze the selected executable immediately, and record its hash before running
the TLS audit.

The paired runs must agree on installed files and both certificate-rejection
outcomes for each TLS version. This gate does not measure performance. Passing
the prebuilt-binding reference fixture does not establish that the Toucan uv
build has passed; that requires the actual generated build and its Cargo audit.
