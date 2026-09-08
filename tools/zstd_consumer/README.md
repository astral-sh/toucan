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

The harness builds and runs the consumer twice: first with upstream's checked-in
bindings, then with fresh, unmodified Toucan output in the binding files selected
by upstream's `src/lib.rs`. The crate root, C build script, library sources, and
Rust wrappers remain unchanged. It checks the locked package sources, checksums,
and resolved features, then compares the runtime results. The dependency graph
has no bindgen or libclang requirement.

JSON evidence records commands, generated bindings, package and source hashes,
and rustc dependency files proving that both selected generated binding files
were compiled. Only those two files may differ from the upstream sys crate.
This is a native execution check: `--target` must match the Rust compiler's host.

## Feature profiles

The default invocation keeps zstd's default Cargo features. Repeat `--profile`
to run a matrix; CI runs all four profiles with current Rust and Rust 1.64:

```console
python3 scripts/verify_zstd_consumer.py \
  --toucan target/release/toucan \
  --target x86_64-unknown-linux-gnu --sysroot / \
  --profile default --profile experimental --profile zstdmt \
  --profile experimental-zstdmt \
  --output corpus/results/zstd-consumer
```

| Profile | Additional runtime checks |
| --- | --- |
| `default` | Bulk and checksum-enabled streaming round trips; dictionary training, prepared dictionary compression/decompression, dictionary IDs, and rejection without the dictionary |
| `experimental` | Magicless frames with matching decoder configuration; experimental COVER dictionary training, passing a nested parameter record by value |
| `zstdmt` | Two-worker streaming compression over almost six MiB with one-MiB jobs and a checksum |
| `experimental-zstdmt` | All of the above plus a shared thread pool; frame progression reports multiple completed jobs and exact input/output byte counts |

Each profile runs the default checks too. Every compressed frame and trained
dictionary is saved and compared byte for byte between upstream and Toucan runs;
JSON evidence records their SHA-256 hashes. Worker completion timing is not compared.
The experimental profile generates `bindings_zstd_experimental.rs` and
`bindings_zdict_experimental.rs`; the other profiles generate `bindings_zstd.rs`
and `bindings_zdict.rs`. Experimental generation uses the upstream defines
`ZSTD_STATIC_LINKING_ONLY`, `ZDICT_STATIC_LINKING_ONLY`, and
`ZSTD_RUST_BINDINGS_EXPERIMENTAL`. No bindings are concatenated or edited afterward.

The inspected Ruff manifests enable zstd through
[`ty_project` → `ty_vendored` → `zip/zstd`](https://github.com/astral-sh/ruff/blob/e7adf82ff005f3ab3051c363464cf65bf8a6e2f3/crates/ty_vendored/Cargo.toml).
The inspected uv manifests enable it through
[`async-compression`, `astral_async_zip`, and `reqwest`](https://github.com/astral-sh/uv/blob/d28a3ee3d0f7122b0da64b0226d2e173e7d23747/Cargo.toml).
Neither workspace requests `experimental` or `zstdmt` in those manifests; the
corresponding locked dependencies forward ordinary zstd support. This manifest
inspection is not a build of the complete Ruff or uv feature graph. The fixture
tests zstd's defaults plus the explicit profiles above, independently of those
workspace choices.

The [upstream generation profile](https://github.com/gyscos/zstd-rs/blob/434ca4cb364e8a81846a2d99d430977e07b15a52/zstd-safe/zstd-sys/build.rs#L17)
requests Rust enums and `size_t` as `usize`. The wrappers also expect unsigned
nonnegative macro constants. Without these representation options, ABI-equivalent
integer aliases cannot compile those Rust callers. Toucan exposes the options as
`--rustified-enums`, `--size-t-is-usize`, and `--macro-type unsigned`; its default
representation preserves C integer expression types.

The independently generated files use `--helper-namespace zstd` and
`--helper-namespace zdict` to keep anonymous types and layout-test names distinct
within upstream's module. This option preserves public C names and record-local
fields. With `--size-t-is-usize`, an implicit `size_t` dependency outside the
allowlist is lowered directly to `usize`; an explicitly selected `size_t` alias
is still emitted. Other shared public types still require caller coordination
when combining separately generated files.

Seekable APIs and other zstd feature combinations still need separate coverage.
These runs demonstrate the checked Rust wrappers, layouts, and executed API
paths; they are not complete zstd API coverage or a throughput benchmark.

## Rust 1.64

Generation requests `--rust-target 1.64`, matching upstream's build script. Every
generated field-offset test runs with the compiler used by the consumer; size and
alignment assertions remain compile-time checks. To use an installed Rust 1.64
toolchain, add `--rust-toolchain 1.64.0` to either command above. Every profile
builds and runs the same optimized consumer against both sets of bindings.

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
