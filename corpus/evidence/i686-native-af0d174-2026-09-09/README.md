# i686 GNU Linux native layout and FFI

The [GitHub run](https://github.com/astral-sh/toucan/actions/runs/34362089043)
passed at commit `af0d174122e207b91fded865cb2b1f1ad6422329` on Ubuntu
24.04. The 75-second job installed 32-bit glibc development files and Rust's
`i686-unknown-linux-gnu` standard library. It used GCC 13.3, Clang 18.1,
and Rust 1.98.1 on an x86-64 host. The [workflow](../../../.github/workflows/i686-native.yml)
has a 15-minute limit and triggers only on its dedicated acceptance branch;
it has no macOS job.

Toucan generated bindings in both compiler profiles with no omitted
declarations, functions, or macros. GCC and Clang each compiled and ran C
layout assertions for 32-bit pointers and `long`, 12-byte x87 `long double`,
record offsets, packing, and bitfields. Each compiler's C object was linked to
an `i686-unknown-linux-gnu` Rust program using those generated bindings.
Native Rust-to-C calls exercised struct arguments and returns, a struct
callback, stack arguments, packed records, bitfields, and constants for 1,000
rounds with both C and Rust at `-O0` and `-O2`: four passing executions.
Ten artifacts were verified to be ELF32 Intel 80386. The previously generated
[untouched zstd header bindings](../i686-target-2026-09-09/README.md) also
compiled to 32-bit Rust metadata for each compiler profile.

[artifact.zip](artifact.zip) is the exact GitHub Actions artifact upload:
SHA-256 `32d387ef18e2281370cd42c395912b3694d265427153f89e1a2dd9b0dea7ae25`,
matching GitHub's digest for artifact `10108294023`. It preserves all 112
files after the upstream artifact's 14-day retention period, including the C
and Rust sources, generated bindings, tool versions, ELF binaries, probe logs,
and compiled zstd metadata. We checked every file's size and hash in
[summary.json](summary.json), all 40 recorded command exit codes and
stdout/stderr hashes, the source and script digests, generated bindings and
metadata, and the architecture and hashes of the ten ELF artifacts.

The native FFI calls use a focused ABI fixture. This result does not test
Toucan against arbitrary 32-bit glibc headers, invoke zstd through FFI,
run full Ruff/uv builds on i686, or establish nondefault 32-bit calling
conventions. It does not establish a result on other Linux distributions
or any macOS architecture.
