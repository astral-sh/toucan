# Documentation

## Getting started

- [Command-line usage](usage.md): installation, header analysis, system headers, and cross-compilation.
- [Generate bindings](bindings.md): name selection, macro constants, Rust versions, and build scripts.
- [Use the library](library.md): embed the frontend in a Rust application.

## Reference

- [Compatibility](compatibility.md): supported C features, targets, and known gaps.
- [Compiler profiles](compiler-profiles.md) and [language modes](language-modes.md).
- [Analysis API](analysis-api.md) and [JSON inspection](inspection.md).
- [Preprocessing](../crates/toucan_preprocessor/README.md) and [include search](include-search.md).
- [Parser resource limits](parser-limits.md).

## Existing Rust projects

- [Build-script adapter](../crates/toucan_bindgen/README.md): the supported `bindgen::Builder` API.
- [External bindgen executable](external-bindgen-cli.md): AWS-LC build-script integration.
- [Replacement readiness](replacement-readiness.md): tested consumers and release blockers.
- [uv and ty integration](opt-in-rollout.md): proposed opt-in builds and reproduction steps.

## Development and validation

- [Development](development.md): workspace checks and acceptance criteria.
- [Architecture](architecture.md): crate responsibilities and component boundaries.
- [Validation](validation.md): recorded native and integration results.
- [Conformance](conformance.md) and [upstream corpus](../corpus/README.md).
- [Benchmarks](benchmarks.md) and [fuzzing](../fuzz/README.md).
