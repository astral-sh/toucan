# AWS-LC FIPS through the external `bindgen` executable

On native Linux x86-64, Toucan built the pinned `aws-lc-fips-sys 0.14.2` and
`aws-lc-rs 1.18.0` crates through their **unchanged upstream build scripts**.
The only edits were to the copied consumer fixture: select `aws-lc-rs/fips` and
`aws-lc-fips-sys/fips,bindgen`, alias the sys dependency to the existing Rust
consumer name, and call the module's integrity function. The downloaded FIPS
crate matches all 2,011 files in its registry archive, whose SHA-256 equals the
fixture lockfile checksum. The separate `aws-lc-sys 0.44.0` crate used for the
SSL rejection matches all 2,012 files in its archive.

We compiled the standalone `bindgen` executable from Toucan parent `8272530`
with the FIPS linker-symbol correction in this change. A wrapper named
`bindgen` ahead of `PATH` recorded the upstream script's successful `--version`
and 29-argument generation calls. The arguments include
`--prefix-link-name aws_lc_fips_0_14_2_`, `--rust-target 1.70`, and the untouched
FIPS `include/rust_wrapper.h`. The trace records the executable's SHA-256,
`ed638c58cdee0786beb4975a6a43fe2353e73d803a0a46422f80153aff192b31`,
and the resulting 1,057,663-byte bindings SHA-256,
`006c20ddc974bda3d17695084494245cda91358c190f467e0676d43c52dbd912`.
The generated file has that hash; Cargo's Rust dep-info names the same
`OUT_DIR/bindings.rs`, and the upstream build script reports external generation
and `use_bindgen_pregenerated`. These are independent checks that Cargo compiled
the fresh standalone-executable output, rather than bundled pregenerated Rust.

The unchanged upstream FIPS prefix-symbol list deliberately omits
`BORINGSSL_integrity_test`, and `nm` finds only its **unprefixed** definition in
the native FIPS crypto archive. The upstream pregenerated Rust also leaves it
unprefixed. Both Toucan and bindgen-cli originally prefixed its linker name,
which fails when a consumer calls it. Toucan now checks the upstream FIPS symbol
list and preserves the actual linker name. [`native-link-symbols.json`](native-link-symbols.json)
records the FIPS archive hash, real integrity/self-test symbols, and all 60
prefixed global definitions. The native Rust consumer called
`aws_lc_sys::BORINGSSL_integrity_test()` and received `1`. It also passed the
SHA/HMAC cases, 27 AEAD round trips and 108 rejections, Ed25519 and P-256 cases;
all **41 deterministic artifacts** match the existing non-FIPS native crypto
reference byte for byte. Six Rust record sizes and alignments match an
independently compiled C11 probe. The freshly generated Rust passes **97/97**
runtime layout tests.

Using the *same* upstream argument list (only `--output` changed), bindgen-cli
0.72.1 produced the archived reference bindings. The parsed/native differential
agrees on **2,618/2,618 shared functions**, **3,864/3,864 constants**, **97/97
shared record layouts**, and **441/441 shared field offsets**. One function is
keyed differently because Toucan links the actual unprefixed FIPS integrity
symbol; bindgen-cli does not. All 60 globals are also keyed differently:
bindgen-cli leaves them unprefixed despite `--prefix-link-name`, whereas
Toucan's 60 prefixed symbols all exist in the linked FIPS archive. Exact
generated Rust API equality is false: Toucan adds 13 aliases; three record
source shapes and bindgen's extra private storage fields differ. The full
inventory, native observations, and exceptions are in
[`differential.json.gz`](differential.json.gz).

An independent negative build uses unchanged `aws-lc-sys 0.44.0` with `ssl`,
`all-bindings`, and `AWS_LC_SYS_EXTERNAL_BINDGEN=1`. The upstream script
explicitly says `External bindgen required, but external bindgen unable to
produce SSL bindings.` and exits 101 *before calling the executable*. Toucan's
external CLI cannot enable this SSL path without an upstream build-script
change. The earlier [SSL Builder-adapter proof](../aws-lc-ssl-consumer-7db2b85-2026-09-09/README.md)
is a different, working route. `ssl-fixture-Cargo.toml` and
`ssl-fixture-Cargo.lock` preserve the rejected feature selection.

[`summary.json`](summary.json) checksums every archived input and output. The
archive includes both unchanged crate lockfile identities, copied consumer
fixture, trace wrapper and argv, full compressed build and rejected-SSL logs,
both generated binding files, native consumer and C probe outputs, 41 hashed
runtime artifacts, and the complete Rust API differential. Reproduce the FIPS
build by copying `fixture-Cargo.toml`, `fixture-Cargo.lock`, and `fixture-main.rs`
to a standalone Cargo fixture, compiling the standalone Toucan `bindgen`, and
setting `AWS_LC_FIPS_SYS_EXTERNAL_BINDGEN=1`, `PATH` to that executable,
`BINDGEN_EXTRA_CLANG_ARGS='-I /usr/lib/llvm-18/lib/clang/18/include --sysroot /'`,
`CC=clang-18`, `CXX=clang++-18`, and equivalent `CFLAGS` before
`cargo build --locked --offline --release`. `bindgen-wrapper.py` can be copied
to `bindgen` ahead of `PATH` to capture calls with `TOUCAN_BINDGEN_BINARY` and
`TOUCAN_BINDGEN_TRACE` set. The recorded paths in the trace must be adjusted
to the replay checkout and Cargo registry.

This result establishes the selected native Linux x64 FIPS *build and calls*;
it does not certify a FIPS module, test every exported C function or a full TLS
handshake, establish exact Rust API equality, or cover other targets. The
upstream FIPS manifest still compiles bindgen and clang-sys dependencies while
delegating generation to the external executable. Its script also calls
`bindgen::clang_version()` before the external fallback (recorded in the build
log), so this unchanged upstream path still requires libclang. Toucan's
standalone executable itself does not use libclang.
