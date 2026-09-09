# AWS-LC external `bindgen` executable on Linux x86-64

The unchanged `aws-lc-sys` 0.44.0 build script selected the standalone Toucan
`bindgen` executable through `AWS_LC_SYS_EXTERNAL_BINDGEN=1`. The unchanged
`tools/aws_lc_consumer` fixture selected `aws-lc-rs` 1.18.0 with `aws-lc-sys`,
`bindgen`, and `prebuilt-nasm`, without SSL, FIPS, or `all-bindings` features.
The build script reported `Generating bindings - external bindgen` and set
`use_bindgen_pregenerated` for its newly generated `OUT_DIR/bindings.rs`.

The clean release build and native consumer passed: two SHA-256 known answers,
ten streaming lengths, the HMAC known answer and invalid tags, 27 AEAD round
trips and 108 rejections, Ed25519 and P-256 operations, and six C/Rust size and
alignment probes. All 98 generated layout tests passed. The consumer wrote 41
deterministic artifacts whose individual SHA-256 values equal the archived
[paired bindgen reference](../aws-lc-consumer-paired-2026-09-09.json.gz).
The original AWS-LC crate archive SHA-256 matches the fixture lockfile checksum.

[`summary.json`](summary.json) records command inputs, upstream source
checksums, toolchains, output hashes, feature selection, and artifact
comparisons. The original generated file is in [`bindings.rs.gz`](bindings.rs.gz); the 41 native
outputs are in [`artifacts.tar.gz`](artifacts.tar.gz). The exact generated file
SHA-256 is `2275435036f0b389ae7ff54e972348aff18debabfa53ab1a0eb4cb9d796f6caa`;
the unchanged build script and a direct CLI invocation on the pinned headers
produced the same bytes.

The [consumer output](consumer.stdout) and independently [C-compiled layout
probe](c-layout.stdout) agree for all six checked types. The build-script
selection is recorded in [build-script-selection.txt](build-script-selection.txt),
and [layout-tests.log](layout-tests.log) records each of the 98 passing generated
tests. The archived 41 output files and their SHA-256 values are verified against
the earlier bindgen reference in `summary.json`.

Reproduce with `clang-18`, LLVM 18's resource headers, Rust 1.98.1, and the
native Linux x86-64 C sysroot:

```sh
cargo build --locked --offline -p toucan_cli --bin bindgen
export PATH="$(pwd)/target/debug:$PATH"
export AWS_LC_SYS_EXTERNAL_BINDGEN=1
export BINDGEN_EXTRA_CLANG_ARGS='-I /usr/lib/llvm-18/lib/clang/18/include --sysroot /'
export CC=clang-18
export CFLAGS="$BINDGEN_EXTRA_CLANG_ARGS"
export CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/tmp/toucan-external-consumer-target
cargo build --locked --offline --release --manifest-path tools/aws_lc_consumer/Cargo.toml
mkdir -p /tmp/toucan-external-consumer-artifacts
TOUCAN_CONSUMER_OUTPUT=/tmp/toucan-external-consumer-artifacts \
  "$CARGO_TARGET_DIR/release/toucan-aws-lc-consumer"
cargo test --locked --offline --release \
  --manifest-path tools/aws_lc_consumer/Cargo.toml -p aws-lc-sys --lib
```

[`differential.json.gz`](differential.json.gz) compares the CLI-generated file
with [bindgen-cli 0.72.1 output](bindgen-cli-reference.rs.gz) from the **same
argv**, target headers, Clang arguments, Rust target, and formatting. All
2,618 shared functions, 3,851 constants, 98 common native record layouts,
and 441 field offsets agree. Full API equality is false: bindgen-cli leaves
all 60 globals unprefixed despite `--prefix-link-name`, while Toucan attaches
the prefix. `nm` found [all 60 prefixed definitions](native-global-symbols.txt)
in the actual AWS-LC crypto archive. The [Rust link probe](global-link-probe.rs)
with [recorded results](global-link-evidence.json) loads Toucan's generated
bindings and successfully resolves an AWS-LC global; the same program using
bindgen-cli output fails to link because `ASN1_BOOLEAN_it` is undefined. The
remaining source differences are 13 extra Toucan aliases and four record shapes
(synthetic opaque storage and three private padding fields in bindgen output).

This evidence is native Linux x86-64 crypto-only validation. It does not cover
SSL, FIPS, other targets, general bindgen-cli compatibility, or the later stack
head. The original AWS-LC manifest still compiles its `bindgen`, `clang-sys`,
and `libloading` build dependencies even when it delegates generation to the
external executable.
