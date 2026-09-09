# Command-line usage

## Installation

After a binary release has been published, install the latest release on macOS
or Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/astral-sh/toucan/releases/latest/download/toucan-installer.sh | sh
```

On Windows, run in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/astral-sh/toucan/releases/latest/download/toucan-installer.ps1 | iex"
```

Alternatively, download an archive and its SHA-256 checksum from
[GitHub Releases](https://github.com/astral-sh/toucan/releases). Each release
includes x86-64 and ARM64 builds for macOS, GNU/Linux, and Windows (MSVC).
For a specific version or prerelease, use the installer linked from that release
instead of the `latest` URL.

Binary releases install the `toucan` executable and use the system allocator.
The experimental `bindgen` adapter is installed separately, as described
[below](#alternative-executables-and-allocators).

To build from source, install from the repository root with Rust 1.96 or later:

```console
cargo install --path crates/toucan_cli --bin toucan --locked
```

The frontend does not require libclang or invoke a C compiler. Target system headers
and compiler resource headers may still be needed to parse an application's headers.
Linking and using the generated bindings requires the corresponding C library.

See [binding generation](bindings.md) for `toucan bindgen`, or run
`toucan help` and `toucan help <command>` for the available options.

## Analyze headers

Each command accepts `--target`, `--compiler`, `--std`, `--sysroot`, `-I`, `-D`, and `-U`.
Use `--compiler clang` for Clang on Linux; omitted selection preserves the target
default. [Language modes](language-modes.md) select C90, C99, C11, or C17,
with an ISO or GNU mode for each standard. GNU11 is the default.
See [compiler profiles](compiler-profiles.md) for supported pairs.

```console
toucan preprocess api.h --output api.i
toucan check api.h
toucan inspect api.h --output api.json
toucan inspect api.h --checked-code --output checked-api.json
```

`preprocess` expands macros and includes. `check` validates declarations and function
bodies within the supported scope. Unsupported constructs produce diagnostics.
`inspect` writes the semantic representation as versioned JSON. Add `--checked-code`
to include typed bodies, expressions, initializers, and source mappings; see
[checked inspection](inspection.md). The library API and JSON schema are experimental.

## Reproducible output

The CLI captures one UTC timestamp for `__DATE__` and `__TIME__`. Set
`SOURCE_DATE_EPOCH` to Unix seconds for reproducible output; malformed values fail
before writing output. For example, `SOURCE_DATE_EPOCH=0 toucan preprocess api.h`
uses `"Jan  1 1970"` and `"00:00:00"`. `TZ` and locale do not change these macros.

## System headers and cross-compilation

Toucan uses the selected target's data model and predefined macros, independently
of the host. Supply headers for that target through `--sysroot` and ordered `-I`
arguments. `--sysroot` adds `usr/include` and, on Linux, the target's multiarch
include directory; it does not discover a compiler installation or SDK. The
[x86-64 and AArch64 musl targets](musl.md) require musl headers and preserve
the libc environment in generated Rust target guards.

For native Linux headers, `--sysroot /` selects the installed system headers. Add
compiler resource directories with `-I` when needed. On macOS, use the installed SDK:

```console
toucan check api.h --target aarch64-apple-darwin \
  --sysroot "$(xcrun --show-sdk-path)"
```

The shell invokes `xcrun` in this example. Toucan itself does not launch it. Choose
`x86_64-apple-darwin` for Intel macOS. Bundled fallback headers cover `stddef.h`,
`stdarg.h`, `stdbool.h`, and `limits.h`; they are not a complete C library or SDK.
See the [compatibility matrix](compatibility.md#targets) for target coverage.

## Alternative executables and allocators

To install the separate `bindgen` executable for supported AWS-LC build scripts:

```console
cargo install --path crates/toucan_cli --bin bindgen --locked
```

See the [external bindgen guide](external-bindgen-cli.md) for its supported arguments
and build-script requirements.

The CLI uses the system allocator by default. Build with
`--features performance-allocator` to use jemalloc on supported Unix platforms or
mimalloc on Windows.
