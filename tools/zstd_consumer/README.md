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

This checks current Rust compilation on the native corpus targets. The upstream
generator also requests Rust 1.64 output; compatibility with that older compiler
is not established by this harness. Consumer paths using zstd's experimental,
seekable, or multithreaded features need separate coverage.
