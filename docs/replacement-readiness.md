# Replacement readiness

Toucan is experimental. Its Builder implements a subset of bindgen's API;
the public Rust API and inspection JSON have no stable compatibility commitment.
A successful integration applies to its tested library version, target,
compiler profile, headers, features, and binding options.

## Consumer checks

| Consumer | Maintained entry point | Scope to verify |
| --- | --- | --- |
| zstd-sys, zstd-safe, zstd | [zstd consumer](../tools/zstd_consumer/README.md) | Default, experimental, and threaded features; generated layouts and compression/decompression behavior. |
| libsqlite3-sys, rusqlite | [SQLite consumer](../tools/sqlite_consumer/README.md) | Bundled SQLite, queries, callbacks, and serialization. |
| aws-lc-sys, aws-lc-rs | [AWS-LC consumer](aws-lc-consumer.md) | Crypto, all-bindings, and SSL are separate configurations; preserve their original build-script options. |
| Ruff/ty and uv | [Application integration](astral-consumers.md) | Build-script replacement and selected application tests; full builds and suites must be requested explicitly. |
| uv HTTPS | [TLS consumer](uv-tls-consumer.md) | Selected TLS versions, payloads, and certificate rejection behavior. |
| Untouched public headers | [Upstream corpus](../corpus/README.md) | zlib, SQLite, zstd, and libgit2 declarations, constants, layouts, and FFI calls. |

The [opt-in integration guide](opt-in-rollout.md) describes reproducible local
uv/ty trials. It does not imply those changes are present in upstream consumers.
Historical outcomes are available through the [validation archive](validation.md#historical-results).

## Adoption gates

1. Select a supported [target and compiler profile](compatibility.md#targets).
   Supply the same headers, relevant defines, and ABI configuration used to build
   the C library. PPC64LE, RISC-V64, and s390x remain unsupported. ARMv7 supports
   the Clang hard-float profile and requires Rust 1.78 output or later.
2. Generate and compile the actual consumed bindings with the intended Rust
   version. Inspect omissions and API differences; layout equality does not
   establish function-call compatibility or that an existing wrapper compiles.
3. Run the original consumer build, independent C/Rust layouts, and representative
   native calls for every shipping target and feature configuration. Repeat on
   the revision being adopted. Cross-target headers and layout probes do not
   establish full application compatibility.
4. Retain a fallback for unsupported configurations. Unknown ABI attributes and
   unsupported selected representations should remain explicit errors.

For AWS-LC's external executable route, upstream build scripts can retain
libclang-dependent build dependencies or version queries. The unmodified
`aws-lc-sys 0.44.0` script rejects external SSL before invoking the generator;
the FIPS route still calls `bindgen::clang_version()`. Replacing the executable
alone therefore does not remove every libclang dependency. See the
[external executable guide](external-bindgen-cli.md).
