# Toucan

A C frontend and Rust binding generator, written in Rust.

[**Documentation**](docs/README.md) | [**Compatibility**](docs/compatibility.md) | [**Benchmarks**](docs/benchmarks.md)

Toucan is experimental and is not yet a production-ready replacement for bindgen.

## Highlights

- Generate Rust bindings without libclang or a C compiler.
- Preprocess, type-check, and inspect C headers from the command line.
- Evaluate constants and compute type layouts for an explicit target, independently of the host.
- Embed the frontend in your own tools through reusable Rust libraries.

## Installation

Build from this checkout with Rust 1.96 or later:

```console
cargo install --path crates/toucan_cli --bin toucan --locked
```

Headers that include system or compiler headers need the corresponding
[sysroot and include paths](docs/usage.md#system-headers-and-cross-compilation).

## Getting started

Given a C header, `api.h`:

```c
#define API_VERSION 1

typedef struct api_point {
    double x;
    double y;
} api_point;

int api_translate(api_point *point, double dx, double dy);
```

Generate Rust bindings:

```console
toucan bindgen api.h --target x86_64-unknown-linux-gnu \
  --allowlist 'api_*' --allowlist 'API_*' --output bindings.rs
```

The generated records include compile-time layout checks. Add `--report bindings.json`
to record dependencies, timings, and omitted declarations or macros.
See the [binding guide](docs/bindings.md) for output options and
[build-script integration](docs/bindings.md#existing-binding-build-scripts).

Toucan can also preprocess, check, and inspect headers:

```console
toucan preprocess api.h --output api.i
toucan check api.h
toucan inspect api.h --output api.json
```

See the [command-line guide](docs/usage.md) for target configuration and header analysis,
or the [library guide](docs/library.md) to use Toucan from Rust.

## Contributing

See the [development guide](docs/development.md) to get started and the
[architecture guide](docs/architecture.md) for an overview of the crates.
[Validation results](docs/validation.md) document the tested configurations and remaining gaps.

## License

Toucan is licensed under either the [Apache License, Version 2.0](LICENSE-APACHE),
or the [MIT license](LICENSE-MIT), at your option.
