# Toucan

A C frontend in Rust. Generate Rust bindings without libclang, or embed the frontend
in your own tools.

Toucan provides preprocessing, declaration analysis, integer constant evaluation,
and target-specific layout through reusable libraries. Rust binding generation is
the first application; the same representation is intended for API compatibility
checks, header indexes, and static analysis.

**Toucan is experimental.** It analyzes header declarations, but does not yet
type-check function bodies or variable initializers. It is not a production-ready
replacement for bindgen. See [compatibility](docs/compatibility.md) for supported
features and remaining gaps.

## Installation

Build from this checkout with Rust 1.96 or later:

```console
cargo install --path crates/toucan_cli --locked
```

The frontend does not require libclang or invoke a C compiler. Target system headers
and compiler resource headers may still be needed to parse an application's headers.
Linking and using the generated bindings requires the corresponding C library.

## Generate bindings

Given `api.h`:

```c
#define API_VERSION 1

typedef struct api_point {
    double x;
    double y;
} api_point;

int api_translate(api_point *point, double dx, double dy);
```

Generate declarations and macro constants for an explicit target:

```console
toucan bindgen api.h --target x86_64-unknown-linux-gnu \
  --allowlist 'api_*' --allowlist 'API_*' \
  --output bindings.rs --report bindings.json
```

An allowlist entry selects an exact C name or a prefix ending in `*`. Referenced
types are included automatically. With no allowlist, all names are selected.
Compiler-internal macros beginning with `__` are omitted by default unless they
shadow a declaration. An explicit matching allowlist selects them.
Generated records include compile-time size, alignment, and field-offset assertions.
The output also checks that Rust is compiling for the selected target.

The JSON report records dependencies, phase timings, and omitted declarations and
macros. Unsupported selected ABI representations fail generation. Macros that cannot
be represented as integer or ordinary string constants are reported as omitted;
`--deny-skipped-macros` makes these omissions an error. Function-like macros are not
translated into Rust functions.

An object macro replaces a same-named enum constant in the generated bindings.
Self-aliases such as `#define VALUE VALUE` retain the enum representation. Macros
that conflict with other declaration names produce a diagnostic. The report maps
renamed Rust macro constants back to their original C names.

## Analyze headers

Each command accepts `--target`, `--sysroot`, `-I`, `-D`, and `-U`:

```console
toucan preprocess api.h --output api.i
toucan check api.h
toucan inspect api.h --output api.json
```

`preprocess` expands macros and includes. `check` validates header declarations
within the supported scope. `inspect` writes the semantic representation as versioned
JSON. The library API and JSON schema are experimental.

### System headers and cross-compilation

Toucan uses the selected target's data model and predefined macros, independently
of the host. Supply headers for that target through `--sysroot` and ordered `-I`
arguments. `--sysroot` adds `usr/include` and, on Linux, the target's multiarch
include directory; it does not discover a compiler installation or SDK.

For native Linux headers, `--sysroot /` selects the installed system headers. Add
compiler resource directories with `-I` when needed. On macOS, use the installed SDK:

```console
toucan check api.h --target aarch64-apple-darwin \
  --sysroot "$(xcrun --show-sdk-path)"
```

The shell invokes `xcrun` in this example. Toucan itself does not launch it. Choose
`x86_64-apple-darwin` for Intel macOS. Bundled fallback headers cover `stddef.h`,
`stdarg.h`, `stdbool.h`, and `limits.h`; they are not a complete C library or SDK.
See the [compatibility matrix](docs/compatibility.md#targets) for target coverage.

## Use the library

Use `crates/toucan` as a Cargo path dependency. This example preprocesses an in-memory
header with filesystem access disabled and generates bindings:

```rust
use std::path::Path;

use toucan::{BindingOptions, Config, Target};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.allow_filesystem = false;

    let compilation = toucan::parse_source(
        Path::new("api.h"),
        "typedef struct api_point { double x, y; } api_point;",
        &config,
    )?;
    let (bindings, report) = compilation.bindings(&BindingOptions {
        allowlist: vec!["api_*".into()],
    })?;

    assert!(report.skipped_declarations.is_empty());
    println!("{bindings}");
    Ok(())
}
```

`Compilation::unit` exposes the declarations and types for other consumers. Use
`parse_file` for files, and configure include directories, predefined macros, virtual
headers, and resource limits through `Config::preprocessor`.

Library crates forbid unsafe Rust, do not invoke compiler processes, and leave
allocator selection to the embedding application. The CLI uses the system allocator
by default. Build with `--features performance-allocator` to use jemalloc on supported
Unix platforms or mimalloc on Windows.

## Validation

The [upstream corpus](corpus/README.md) builds pinned releases of zlib, SQLite, zstd,
and libgit2 and processes their untouched public headers. Native runs on x86_64 and
AArch64 Linux and macOS passed 5,444 C/Rust comparisons per target and actual FFI
calls into all four libraries.

The same runs matched 1,284 function signatures and three global types with bindgen
and independently checked every complete generated record against C. Depending on
the target, that covered 109–111 records and 628–638 ordinary field offsets. The
comparison gate passed with no unexplained differences; exact API equivalence
remains false, with each accepted difference recorded and justified.

The [recorded evidence](corpus/evidence/native-equivalence-2026-09-08.json) identifies
the tested commits and configurations. See [compatibility](docs/compatibility.md)
for coverage and gaps. [Benchmarks](docs/benchmarks.md) and [fuzzing](fuzz/README.md)
record separate performance and malformed-input checks.

## Development

| Crate | Responsibility |
| --- | --- |
| [`toucan_source`](crates/toucan_source) | Source files and locations |
| [`toucan_target`](crates/toucan_target) | Target profiles and layout |
| [`toucan_preprocessor`](crates/toucan_preprocessor) | Native C preprocessing |
| [`toucan_semantic`](crates/toucan_semantic) | Parsing, declaration semantics, and constant evaluation |
| [`toucan_bindings`](crates/toucan_bindings) | Rust binding generation |
| [`toucan`](crates/toucan) | Integrated library and reports |
| [`toucan_cli`](crates/toucan_cli) | Command-line interface |

```console
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

See [architecture](docs/architecture.md) for component boundaries and
[development](docs/development.md) for acceptance criteria. C compilers are used by
validation tools as an independent reference.

## License

Toucan is licensed under either the [Apache License, Version 2.0](LICENSE-APACHE),
or the [MIT license](LICENSE-MIT), at your option.
