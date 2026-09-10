# Toucan

A C frontend and Rust binding generator, written in Rust.

| Project | Toucan | bindgen 0.72.1 | Speedup |
| --- | ---: | ---: | ---: |
| zlib 1.3.1 | 22.22 ms | 117.93 ms | 5.30× |
| SQLite 3.45.1 | 25.36 ms | 154.92 ms | 6.10× |
| zstd 1.5.7 | 9.12 ms | 101.91 ms | 11.23× |
| libgit2 1.9.1 | 192.85 ms | 263.19 ms | 1.37× |

> [!WARNING]
> This README is human-edited, but all code changes, PR summaries, and additional
> documentation were authored entirely by GPT-6 Astra in [Codex](https://openai.com/codex/).
> Use at your own risk.

## Highlights

- Generate Rust bindings without libclang or a C compiler.
- Preprocess, type-check, and inspect C headers from the command line.
- Evaluate constants and compute type layouts for an explicit target, independently of the host.
- Embed the frontend in your own tools through reusable Rust libraries.

## Installation

Install a published binary release for macOS, Linux, or Windows from
[GitHub Releases](https://github.com/astral-sh/toucan/releases). Releases include
x86-64 and ARM64 binaries, checksums, and shell and PowerShell installers.
See the [installation guide](docs/usage.md#installation) for commands.

Or build from this checkout with Rust 1.96 or later:

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

## License

Toucan is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Toucan
by you, as defined in the Apache-2.0 license, shall be dually licensed as above, without any
additional terms or conditions.
