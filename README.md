# Toucan

A C frontend and Rust binding generator, written in Rust.

> [!WARNING]
> This README is human-edited, but all code changes, PR summaries, and additional
> documentation were authored entirely by GPT-6 Astra in [Codex](https://openai.com/codex/).
> Use at your own risk.

## Highlights

- Generate Rust bindings without libclang or a C compiler.
- Preprocess, type-check, and inspect C headers from the command line.
- Evaluate constants and compute type layouts for an explicit target, independently of the host.
- Embed the frontend in your own tools through reusable Rust libraries.

| Project | Toucan | bindgen 0.72.1 | Speedup |
| --- | ---: | ---: | ---: |
| zlib 1.3.1 | 17.24 ms | 122.04 ms | 7.05× |
| SQLite 3.45.1 | 17.03 ms | 161.75 ms | 9.52× |
| zstd 1.5.7 | 6.84 ms | 106.66 ms | 15.54× |
| libgit2 1.9.1 | 152.78 ms | 265.29 ms | 1.76× |

We validate generated bindings against GCC and Clang and test native C/Rust calls
on real libraries.

## Installation

Install from Git with Rust 1.96 or later:

```console
cargo install --git https://github.com/astral-sh/toucan toucan_cli --bin toucan --locked
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

This produces the following declarations in `bindings.rs`:

```rust
#[repr(C)]
#[derive(Clone, Copy)]
pub struct api_point {
    pub x: ::core::primitive::f64,
    pub y: ::core::primitive::f64,
}

unsafe extern "C" {
    pub fn api_translate(
        arg0: *mut api_point,
        arg1: ::core::primitive::f64,
        arg2: ::core::primitive::f64,
    ) -> ::core::ffi::c_int;
}
pub const API_VERSION: ::core::primitive::i32 = 1;
```

The generated file also includes compile-time target and layout checks, omitted here.
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

## License

Toucan is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Toucan
by you, as defined in the Apache-2.0 license, shall be dually licensed as above, without any
additional terms or conditions.
