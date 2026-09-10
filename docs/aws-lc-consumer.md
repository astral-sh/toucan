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

The separate [`AWS_LC_SYS_EXTERNAL_BINDGEN=1` path](external-bindgen-cli.md)
has [native Linux x86-64 crypto-only evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/aws-lc-external-cli-317756d/README.md)
from an unchanged upstream build script and all 98 generated layout tests.
The paired consumer runs below use the Cargo Builder adapter; they do not
exercise the standalone executable.

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
The dependency audit removes only the sys crate's binding-generator build edge
and compares the remaining active package versions, sources, checksums, features,
and dependency edges. Generator-only dependencies can disappear; shared native
build dependencies must remain unchanged.

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

## All-bindings profile

The [paired `all-bindings` capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/aws-lc-all-bindings-7db2b85/README.md)
uses the same frozen `7db2b850` frontend with unchanged upstream build scripts
and Rust wrappers. Both builds enable the same sys crate features and run 98
emitted layout tests. The 41 crypto artifacts match each other and the previous
crypto-only capture; six C/Rust layouts and a C/Rust memory-BIO call agree. The
build artifact and dep-info audit confirms the freshly generated bindings were
consumed. This Linux x86-64 run does not enable SSL or FIPS, prove full binding
API equality, or validate another target.

The [full generated-binding differential](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/aws-lc-all-bindings-differential-7db2b85/README.md)
matches 2,618 functions, 60 globals, 3,851 constants, 98 shared compiled Rust
record layouts, and 441 shared field offsets after canonicalizing Linux ELF
linker symbols. Complete public Rust API equality is false: Toucan emits 13
additional aliases, does not expose the same short name for one incomplete
nested tag, and leaves three private padding fields implicit. The direct type
name can matter to downstream Rust code even when function signatures agree.
The subsequent [incomplete-tag name correction](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/incomplete-record-names/README.md)
replays this frozen input and changes only the eight occurrences of that one
public type name; it does not repeat the full AWS-LC consumer build. The other
record-shape and alias differences remain.

## SSL profile on frozen source

The [paired native Linux x86-64 SSL run](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/aws-lc-ssl-consumer-7db2b85-2026-09-09/README.md)
uses the frozen `7db2b85` frontend and unchanged `aws-lc-sys` 0.44.0 and
`aws-lc-rs` 1.18.0 build scripts. With `ssl` and `all-bindings` selected,
both generators' 106 generated layout tests pass. Independently built C and
Rust programs call `TLS_method`, allocate an SSL context and object, check
minimum protocol setters and getters, and free both objects. All 41 recorded
crypto runtime artifacts and six C/Rust layouts agree. Both builds explicitly
use `CXX=clang++-18` because the upstream SSL C++ flags fail with GCC C++.

The ELF-aware differential matches 3,230 functions, 60 globals, 4,894
constants, and all 105 shared record layouts. Exact Rust API equality remains
false for 15 extra aliases, four record field representations, and private
padding or opaque storage. The paired test establishes selected native SSL
calls, not a full TLS handshake or the FIPS and external-executable modes.
The source used in this capture predates the subsequent incomplete-tag name
correction; recheck full consumers on the intended release revision.

## External executable on frozen source

The [native Linux x86-64 external-executable run](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/aws-lc-external-cli-317756d/README.md)
sets `AWS_LC_SYS_EXTERNAL_BINDGEN=1` and places Toucan's standalone `bindgen`
executable on `PATH`. The unchanged `aws-lc-sys` build script generates and
compiles fresh crypto bindings. The resulting consumer matches the 41 archived
crypto artifacts, passes six C/Rust layouts, and passes all 98 generated layout
tests. A same-command comparison matches 2,618 functions and 3,851 constants.

Exact Rust API equality remains false. The reference `bindgen` executable leaves
60 global linker names unprefixed despite `--prefix-link-name`; Toucan prefixes
them, and a native probe links one against the actual library. Thirteen extra
aliases and four record shapes also differ. This run does not cover SSL, FIPS,
other targets, or the later stack head. The unchanged upstream manifest still
compiles `bindgen`, `clang-sys`, and `libloading` as build dependencies even
though it launches Toucan for generation.

## External FIPS executable and external SSL limit

The [native `aws-lc-fips-sys 0.14.2` run](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/aws-lc-external-fips-2026-09-09/README.md)
uses the unchanged upstream FIPS build script and a traced standalone Toucan
`bindgen` executable. The generated file is compiled and consumed by
`aws-lc-rs/fips`: all 97 generated layouts pass, 41 crypto artifacts equal the
prior reference, six layouts match a C11 probe, and a direct native Rust call
to `BORINGSSL_integrity_test` succeeds. The FIPS symbol list leaves that
function unprefixed; Toucan now follows the C definition, unlike the
same-command bindgen-cli output. Full generated API equality remains false.
The build does not establish FIPS certification or other targets. The unchanged
upstream script still calls `bindgen::clang_version()`, so this functional FIPS
path does not remove its libclang requirement.

The same external selection with `aws-lc-sys 0.44.0` and `ssl` fails in the
unchanged upstream script *before* it launches the executable. The earlier
successful SSL run uses the Cargo Builder adapter, a distinct build path.
