# zstd consumer validation

This fixture uses unmodified `zstd` 0.13.3 and `zstd-safe` 7.2.4 with the C library
from `zstd-sys` 2.0.16+zstd.1.5.7. The lockfile pins the full dependency graph.
These are the versions in the inspected [uv lockfile](https://github.com/astral-sh/uv/blob/d28a3ee3d0f7122b0da64b0226d2e173e7d23747/Cargo.lock)
and [Ruff lockfile](https://github.com/astral-sh/ruff/blob/e7adf82ff005f3ab3051c363464cf65bf8a6e2f3/Cargo.lock).

Run from the repository root after building Toucan:

```console
python3 scripts/verify_zstd_consumer.py \
  --toucan target/release/toucan \
  --target x86_64-unknown-linux-gnu --sysroot / \
  --output corpus/results/zstd-consumer
```

The harness copies the pinned sys crate into its cache and replaces its binding
includes with fresh, unmodified Toucan output. The upstream C build script,
library sources, features, and Rust wrappers remain unchanged. It then compiles
the real wrappers and runs both bulk and streaming compression round trips, with
a checksum enabled for the streaming API. The dependency graph has no bindgen or
libclang requirement. JSON evidence records package checksums, source and output
hashes, commands, and results.

The [upstream generation profile](https://github.com/gyscos/zstd-rs/blob/434ca4cb364e8a81846a2d99d430977e07b15a52/zstd-safe/zstd-sys/build.rs#L17)
requests Rust enums and `size_t` as `usize`. The wrappers also expect unsigned
nonnegative macro constants. Without these representation options, ABI-equivalent
integer aliases cannot compile those Rust callers. Toucan exposes the options as
`--rustified-enums`, `--size-t-is-usize`, and `--macro-type unsigned`; its default
representation preserves C integer expression types.

CI checks both the current compiler and Rust 1.64 on the native corpus targets.
Consumer paths using zstd's experimental, seekable, or multithreaded features need
separate coverage.

## Rust 1.64

Generation requests `--rust-target 1.64`, matching upstream's build script. Every
generated field-offset test runs with the compiler used by the consumer; size and
alignment assertions remain compile-time checks. To use an installed Rust 1.64
toolchain, add `--rust-toolchain 1.64.0` to the command above. This also builds and
runs the same optimized consumer with upstream's checked-in bindings as a baseline.

The lock uses Cargo's version 3 format and pins Rust 1.64-compatible build
dependencies: cc 1.4.2, find-msvc-tools 0.1.10, jobserver 0.1.32, libc 0.2.183,
pkg-config 0.3.34, and shlex 2.0.1. The three zstd crate versions are unchanged.
Newer jobserver releases pull target-specific dependencies with newer Rust
requirements. The harness uses current Cargo to vendor the locked dependencies,
then runs Cargo 1.64 offline against that directory, so sparse registry settings
do not require unstable Cargo flags. Source checksums remain checked by Cargo.

CI also runs `scripts/verify_rust_target.py` against all four corpus libraries,
compiling their generated bindings with Rust 1.64 and executing every generated
layout test. The standalone Rust regression exercises packed records, unions,
bitfields, nested records, 128-bit constants, and `core::ffi::CStr`, and proves
that an incorrect expected field offset fails the test.
