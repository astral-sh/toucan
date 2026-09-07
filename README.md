# Toucan

A C frontend in Rust, with Rust binding generation as its first application.

Toucan is being built to preprocess and analyze C headers without libclang. The frontend
libraries accept an explicit target and can be embedded in binding generators, API
compatibility checkers, and header indexes. Target headers and sysroots are still required.

This project is under development. See [the implementation plan](docs/development.md)
for acceptance criteria and the supported scope of each layer. It is not yet a
production-ready replacement for bindgen.

## Development

Toucan uses a Cargo workspace with reusable libraries under `crates/`.

```console
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The library crates forbid unsafe Rust. Libraries do not select a global allocator or
invoke external compiler processes. C compilers are used by validation tools as an
independent reference.

## License

Toucan is licensed under either the Apache License, Version 2.0, or the MIT license,
at your option.

