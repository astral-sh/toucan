# AWS-LC consumer validation

`scripts/verify_aws_lc_consumer.py` builds `aws-lc-rs` 1.18.0 and `aws-lc-sys`
0.44.0 twice: once with bindgen 0.72.1 and once with `toucan_bindgen`. Both builds
generate their bindings through the unchanged upstream build script. The sys
crate's Cargo manifest is the only upstream file changed for Toucan: its optional
binding generator dependency points at the local adapter.

The fixture follows uv's crypto-only feature selection and explicitly enables
binding generation. It retains `prebuilt-nasm`; it does not enable SSL,
`all-bindings`, or FIPS. The generated bindings target Rust 1.70, as requested by
the upstream builder. The Rust wrapper requires Rust 1.71; the harness records
the actual compiler used and does not infer an older compiler result.

## Run

Fetch the fixture's locked dependencies, then use fresh evidence and build
directories:

```sh
cargo fetch --locked --manifest-path tools/aws_lc_consumer/Cargo.toml
python3 scripts/verify_aws_lc_consumer.py \
  --cc clang-18 \
  --sysroot / \
  --libclang-dir /usr/lib/llvm-18/lib \
  --cache /tmp/aws-lc-consumer-build \
  --output /tmp/aws-lc-consumer-evidence
```

The harness currently executes on native GNU/Linux and macOS hosts. Supply the
host's SDK path as `--sysroot` on macOS. `--toucan-source` selects an isolated
adapter checkout. `--reference-only` validates the upstream fixture and reports
`reference_passed`; it does not claim Toucan compatibility.

## Evidence

The harness verifies the downloaded crate archives against Cargo.lock and
checks every archived file against the extracted upstream sources. It records
source hashes before and after the build, both resolved lockfiles, compiler
identities, commands, build output, generated bindings, native executables, and
artifact hashes. Reference and candidate builds have separate source and target
directories to prevent stale Cargo artifacts from being reused.

Both builds use the same explicit C compiler, Clang resource headers, and
sysroot. The native CFLAGS receive those same resource and sysroot arguments.
Cargo's selected dependency tree and compiler-artifact messages verify
the active packages and features. `cargo metadata` alone is insufficient here:
it includes the weak optional FIPS dependency even when that package is not
built. The Toucan build must compile without bindgen, clang-sys, or libloading.

The upstream cfg named `use_bindgen_pregenerated` selects the freshly generated
`OUT_DIR/bindings.rs`. The harness checks that emitted cfg and the sys crate's
Rust dep-info, rejects a shipped `*_crypto.rs` fallback, and verifies the
generator's marker in the consumed bindings. It also compares native object
selection and the names of the bundled static libraries.

## Runtime checks

- SHA-256 empty and `abc` known answers, plus streaming versus one-shot hashing
  at ten lengths around block boundaries.
- The [RFC 4231](https://www.rfc-editor.org/rfc/rfc4231#section-4.2)
  HMAC-SHA-256 known answer, valid verification, and rejection of
  modified messages, modified tags, and truncated tags.
- AES-128-GCM, AES-256-GCM, and ChaCha20-Poly1305: 27 authenticated round trips
  and 108 rejections for changed tags, associated data, nonces, and short inputs.
- The [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032#section-7.1)
  Ed25519 empty-message signing vector, valid verification, and
  four invalid-input checks.
- P-256 ECDSA signing and verification through the unchanged wrapper, with
  rejection of a modified message and truncated signature.
- C-compiled size and alignment checks for `SHA_CTX`, `SHA256_CTX`, `SHA512_CTX`,
  `EVP_AEAD_CTX`, `CBS`, and `CBB`.
- Each generator's complete emitted layout-test suite, compiled and executed
  through the upstream sys crate with the same selected features and bindings.

The reference and Toucan runs must produce identical output and 41 deterministic
artifact hashes. ECDSA signing uses the upstream random generator, so its
signature is verified in each run and excluded from the byte comparison.
These are consumer and FFI checks; they do not establish complete API equality,
full cryptographic test-suite coverage, or performance results.

## Recorded reference

The [frozen reference](../corpus/evidence/aws-lc-consumer-reference-2026-09-08/freeze.json)
passes on Linux x86-64 with bindgen 0.72.1, Clang 18.1.3, and Rust 1.98.1.
It verifies 2,123 upstream files, 362 compiled native objects, 75 emitted layout
tests, and 41 deterministic runtime artifacts. The adjacent archive preserves
the commands, selected Cargo artifacts, generated bindings, and runtime results.
This capture validates the reference fixture; the Toucan run remains pending
the complete adapter API.
