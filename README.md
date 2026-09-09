# Toucan

A C frontend in Rust. Generate Rust bindings without libclang, or embed the frontend
in your own tools.

Toucan provides preprocessing, type checking, arithmetic constant evaluation,
and target-specific layout through reusable libraries. Rust binding generation is
the first application; the same representation is intended for API compatibility
checks, header indexes, and static analysis.

**Toucan is experimental.** It checks declarations, expressions, initializers, and
function bodies within the supported C feature set. It is not yet a production-ready
replacement for bindgen. See [compatibility](docs/compatibility.md) for supported
features and remaining gaps.

## Installation

Build from this checkout with Rust 1.96 or later:

```console
cargo install --path crates/toucan_cli --bin toucan --locked
```

The frontend does not require libclang or invoke a C compiler. Target system headers
and compiler resource headers may still be needed to parse an application's headers.
Linking and using the generated bindings requires the corresponding C library.

For an executable named `bindgen` that accepts the pinned AWS-LC external
binding-generator command, [install the separate CLI](docs/external-bindgen-cli.md):

```console
cargo install --path crates/toucan_cli --bin bindgen --locked
```

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
be represented as integer, `float`/`double`, or string constants are reported as omitted;
`--deny-skipped-macros` makes these omissions an error. Function-like macros are not
translated into Rust functions.

Floating macro expressions are evaluated with the target's C precision and rounding.
Generated `f32` and `f64` constants preserve exact bits, including signed zero,
subnormals, infinities, and NaN payloads. NaN constructors support literal decimal,
octal, and hexadecimal payloads. `long double` and unsupported expressions are
reported as omitted; explicit casts to `float` or `double` are supported.

An object macro replaces a same-named enum constant in the generated bindings.
Self-aliases such as `#define VALUE VALUE` retain the enum representation. Macros
that conflict with other declaration names produce a diagnostic. The report maps
renamed Rust macro constants back to their original C names.

Existing Rust wrappers may require a particular source representation. Use
`--rustified-enums` for named Rust enum variants, `--size-t-is-usize` for `size_t`,
and `--macro-type unsigned` to infer unsigned types for nonnegative macro values.
The default preserves C macro types. Rust enums accept only declared variants;
keep integer aliases for APIs that pass arbitrary values or combine flags.
The report retains original C macro types when a representation option changes
them. The [zstd consumer test](tools/zstd_consumer) exercises these options through
the unmodified `zstd` and `zstd-safe` Rust APIs.

### Existing binding build scripts

The experimental [toucan_bindgen adapter](crates/toucan_bindgen) supports the
builder calls used by the pinned zstd-sys and AWS-LC build scripts. It selects
Cargo's target and generates bindings during the build without libclang. The
adapter documents its supported arguments and API subset; unsupported options
produce errors. See [replacement readiness](docs/replacement-readiness.md) for
the tested consumer paths and release blockers.

The proposed [uv and ty integration](docs/opt-in-rollout.md) adds a build-time
`toucan-zstd` Cargo feature. [Complete builds of both pinned applications](corpus/evidence/astral-git-optin-2026-09-09/README.md)
pass selected library and runtime checks in fresh Linux x86-64 images without
libclang, compiling the frontend from its pinned Git dependency. The pinned
upstream revisions do not expose this selector.

Installing `toucan_cli` also installs a standalone `bindgen` executable. With
that executable first on `PATH`, the pinned `aws-lc-sys` build script selects it
when `AWS_LC_SYS_EXTERNAL_BINDGEN=1`. The [native crypto consumer
proof](corpus/evidence/aws-lc-external-cli-317756d/README.md) checks the unchanged
build script and generated bindings; the executable rejects unsupported options.
This route has not been validated for SSL or FIPS, and the unchanged upstream
manifest still compiles its `bindgen` and `clang-sys` build dependencies.

## Analyze headers

Each command accepts `--target`, `--compiler`, `--std`, `--sysroot`, `-I`, `-D`, and `-U`.
Use `--compiler clang` for Clang on Linux; omitted selection preserves the target
default. [Language modes](docs/language-modes.md) select C90, C99, C11, or C17,
with an ISO or GNU mode for each standard. GNU11 is the default.
See [compiler profiles](docs/compiler-profiles.md) for supported pairs.

```console
toucan preprocess api.h --output api.i
toucan check api.h
toucan inspect api.h --output api.json
toucan inspect api.h --checked-code --output checked-api.json
```

`preprocess` expands macros and includes. `check` validates declarations and function
bodies within the supported scope. Unsupported constructs produce diagnostics. `inspect` writes the semantic representation as versioned
JSON. Add `--checked-code` to include typed bodies, expressions, initializers, and
source mappings; see [checked inspection](docs/inspection.md). The library API and
JSON schema are experimental.

The CLI captures one UTC timestamp for `__DATE__` and `__TIME__`. Set
`SOURCE_DATE_EPOCH` to Unix seconds for reproducible output; malformed values fail
before writing output. For example, `SOURCE_DATE_EPOCH=0 toucan preprocess api.h`
uses `"Jan  1 1970"` and `"00:00:00"`. `TZ` and locale do not change these macros.

### System headers and cross-compilation

Toucan uses the selected target's data model and predefined macros, independently
of the host. Supply headers for that target through `--sysroot` and ordered `-I`
arguments. `--sysroot` adds `usr/include` and, on Linux, the target's multiarch
include directory; it does not discover a compiler installation or SDK. The
[x86-64 and AArch64 musl targets](docs/musl.md) require musl headers and preserve
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
See the [compatibility matrix](docs/compatibility.md#targets) for target coverage.

## Use the library

Library configuration defaults to the Unix epoch for `__DATE__` and `__TIME__` and
never reads a clock or `SOURCE_DATE_EPOCH`. Set `config.preprocessor.timestamp`
with `PreprocessingTimestamp::from_unix_seconds` to choose another value; see
[translation timestamps](crates/toucan_preprocessor/README.md#translation-timestamps).

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
        ..Default::default()
    })?;

    assert!(report.skipped_declarations.is_empty());
    println!("{bindings}");
    Ok(())
}
```

`Compilation::unit()` exposes the declarations and types for other consumers.
Set `Config::analysis.retain_code` to retain typed bodies, expressions, initializers,
and source references through `Compilation::checked()`. See the
[analysis API](docs/analysis-api.md) for ownership, runtime bounds, and source locations. Use
`parse_file` for files, and configure include directories, predefined macros, virtual
headers, and resource limits through `Config::preprocessor`.

The integrated frontend and binding adapter do not invoke a C compiler. The
standalone `toucan_parser::parse` compatibility entry point is an exception: it
launches its configured C preprocessor; `parse_preprocessed` accepts text without
that process. Library crates forbid unsafe Rust and leave allocator selection to
the embedding application. The CLI uses the system allocator by default. Build
with `--features performance-allocator` to use jemalloc on supported Unix
platforms or mimalloc on Windows.

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

The [recorded evidence](corpus/evidence/native-06cefbe/summary.json) identifies
the tested commits and configurations. See [compatibility](docs/compatibility.md)
for coverage and gaps. [Benchmarks](docs/benchmarks.md) and [fuzzing](fuzz/README.md)
record separate performance and malformed-input checks.

The [conformance guide](docs/conformance.md) describes the scope of language,
preprocessor, ABI, and consumer checks. The external C suite now exercises both
compiler-preprocessed input and original source through Toucan's preprocessor;
both routes pass the [Rust 1.96 CI gate](corpus/evidence/native-conformance-ci-2026-09-09/README.md).

The [combined Builder validation](corpus/evidence/native-callbacks-2026-09-09/README.md)
checks the combined Builder and analysis changes: 1,268 workspace tests and
Rustdoc pass on Linux; recorded GitHub jobs also pass on Linux x64/ARM and Windows.
The [Apple Silicon validation at `ac3312d`](corpus/evidence/macos-arm64-ac3312d-2026-09-09/README.md)
passes workspace and package checks, the native corpus, all four zstd profiles,
and a SQLite consumer. It does not validate later commits or full uv/ty builds.
The macOS workflow remains opt-in on pull requests; this run allocated no Intel
runners.

More recent bounded native checks include [installed Windows ARM64 SDK headers](corpus/evidence/windows-arm64-sdk-2026-09-09/README.md)
with generated Rust layouts and an [i686 zstd C/Rust consumer](corpus/evidence/i686-zstd-consumer-2026-09-09/README.md)
with byte-identical compression outputs. They establish those recorded paths;
they do not validate every project configuration or the latest macOS source.

## Development

| Crate | Responsibility |
| --- | --- |
| [`toucan_source`](crates/toucan_source) | Source files and locations |
| [`toucan_target`](crates/toucan_target) | Target profiles and layout |
| [`toucan_preprocessor`](crates/toucan_preprocessor) | Native C preprocessing |
| [`toucan_parser`](crates/toucan_parser) | C grammar and parser AST, derived from lang-c |
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

### Consumer binding policies

`BindingOptions` supports per-name `macro_type_overrides`, a function blocklist,
caller-provided `raw_lines`, and `generate_cstr`. Name patterns are exact names or
prefixes ending in `*`; macro policies prefer exact names, then the longest prefix.
`CStr` generation rejects interior NUL bytes. Raw lines are appended verbatim and
are the caller's responsibility; reports list them separately from analyzed
declarations and record each deliberately blocked function.

The CLI exposes these as `--macro-type-for 'SQLITE_TRACE_*=unsigned'`,
`--blocklist-function NAME`, `--raw-lines-file FILE`, and `--generate-cstr`.
[SQLite consumer validation](tools/sqlite_consumer/README.md) regenerates the
bindings used by pinned, unmodified rusqlite and libsqlite3-sys code, then executes
queries, callbacks, and serialization against the bundled C library.

### Generated Rust versions

Use `--rust-target 1.64`, or `BindingOptions { rust_target:
RustTarget::RUST_1_64, ..Default::default() }`, to select the minimum Rust version
for generated declarations. The default is Rust 1.96. Before Rust 1.82, output
uses legacy extern blocks for Rust editions 2018 and 2021. Before Rust 1.77,
field-offset assertions are generated `#[test]` functions; run `rustc --test`
on the bindings to execute them. Size and alignment remain compile-time checks.

128-bit ABI types require Rust 1.78 with its bundled LLVM, and rustified 128-bit
enums require Rust 1.89. Standalone 128-bit constants work on Rust 1.64. For targets
before 1.78, an empty allowlist omits the reserved `__int128_t` and `__uint128_t`
alias names as selection roots and lists them in `skipped_declarations`.
Explicit selection, or a selected declaration depending on those types, produces
an ABI diagnostic. Reports include `rust_target`; caller-provided raw lines remain
outside this version contract.

The ABI restriction follows Rust's [128-bit compatibility changes](https://blog.rust-lang.org/2024/03/30/i128-layout-update/).
The enum restriction follows the [Rust 1.89 stabilization](https://github.com/rust-lang/rust/pull/138285).
The [zstd fixture](tools/zstd_consumer/README.md) validates both upstream and Toucan
bindings with Rust 1.64 and a pinned compatible dependency lock.

### String macros

Ordinary and `u8` string macros emit byte arrays. `u`, `U`, and `L` strings emit
`u16`, `u32`, or `i32` code-unit arrays according to their prefix and the target's
`wchar_t`. C11 escapes and adjacent literals are supported, including numeric
escapes that do not encode Unicode. Arrays include the implicit terminating NUL;
explicit NULs remain part of the contents.

`--generate-cstr` applies to ordinary and `u8` strings and rejects interior NULs.
Wide strings retain their typed arrays when this option is enabled.


### Inline assembly in headers

GNU basic and extended `asm` statements are checked on the supported Linux and
macOS targets. The frontend validates C operand expressions, writable outputs,
memory addressability, symbolic names, matching constraints, alternative counts,
template references, and a target-specific set of clobbers. Read/write outputs
count twice toward the 30-operand limit. Integer immediates, generic register and
memory constraints, and the x86 `a`, `b`, `c`, `d`, `S`, and `D` register classes
are supported. Unsupported constraint classes and modifiers produce diagnostics.

Machine instructions and register allocation are outside this frontend check,
as described in [GCC's extended asm contract](https://gcc.gnu.org/onlinedocs/gcc/Extended-Asm.html).
Floating-point register classes, asm inline/goto, stack-pointer clobbers, delayed or
address-valued immediates, embedded NUL bytes, and non-UTF-8 assembler text remain
unsupported. The native regressions compare operand acceptance against GCC and
Clang and call optimized C byte-swap wrappers through Toucan-generated bindings.
Apple's byte-order header is covered by the macOS corpus jobs.


## Parser resource limits

The parser bounds work, backtracking steps, recursive rule depth, owned AST depth,
memoized clone cost, and construction metadata. Parsing and semantic traversal use
a bounded worker stack; binding generation shares one scoped session across its
macro evaluations. Large flat function bodies are accepted, and genuinely excessive
inputs return source-positioned resource diagnostics. See [parser limits](docs/parser-limits.md)
for defaults, embedding APIs, C11 nesting coverage, and validation.

### GNU vector types

Fixed-size `vector_size` types through 16 bytes retain their lane types and target
layout. Toucan checks lane operations and emits Rust storage and pointer bindings;
by-value vector calls and wider vectors need additional ABI support. See the
[vector compatibility notes](docs/compatibility.md#fixed-size-gnu-vectors) for the
compiler differences and native C/Rust validation.
