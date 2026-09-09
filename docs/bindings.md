# Generate Rust bindings

Toucan generates Rust bindings from C headers without libclang. Install the CLI
as described in the [README](../README.md#installation), then choose a target and
the C names to include.

## Quickstart

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

Generated records include compile-time size, alignment, and field-offset assertions.
The output also checks that Rust is compiling for the selected target. Linking
and using the bindings requires the corresponding C library.

## Name selection and reports

An allowlist entry selects an exact C name or a prefix ending in `*`. Referenced
types are included automatically. With no allowlist, all names are selected.
Compiler-internal macros beginning with `__` are omitted by default unless they
shadow a declaration. An explicit matching allowlist selects them.

The JSON report records dependencies, phase timings, and omitted declarations and
macros. Unsupported selected ABI representations fail generation. Macros that cannot
be represented as integer, `float`/`double`, or string constants are reported as omitted;
`--deny-skipped-macros` makes these omissions an error. Function-like macros are not
translated into Rust functions.

## Macro constants

Floating macro expressions are evaluated with the target's C precision and rounding.
Generated `f32` and `f64` constants preserve exact bits, including signed zero,
subnormals, infinities, and NaN payloads. NaN constructors support literal decimal,
octal, and hexadecimal payloads. `long double` and unsupported expressions are
reported as omitted; explicit casts to `float` or `double` are supported.

An object macro replaces a same-named enum constant in the generated bindings.
Self-aliases such as `#define VALUE VALUE` retain the enum representation. Macros
that conflict with other declaration names produce a diagnostic. The report maps
renamed Rust macro constants back to their original C names.

### String macros

Ordinary and `u8` string macros emit byte arrays. `u`, `U`, and `L` strings emit
`u16`, `u32`, or `i32` code-unit arrays according to their prefix and the target's
`wchar_t`. C11 escapes and adjacent literals are supported, including numeric
escapes that do not encode Unicode. Arrays include the implicit terminating NUL;
explicit NULs remain part of the contents.

`--generate-cstr` applies to ordinary and `u8` strings and rejects interior NULs.
Wide strings retain their typed arrays when this option is enabled.

## Rust representation

Existing Rust wrappers may require a particular source representation. Use
`--rustified-enums` for named Rust enum variants, `--size-t-is-usize` for `size_t`,
and `--macro-type unsigned` to infer unsigned types for nonnegative macro values.
The default preserves C macro types. Rust enums accept only declared variants;
keep integer aliases for APIs that pass arbitrary values or combine flags.
The report retains original C macro types when a representation option changes
them. The [zstd consumer test](../tools/zstd_consumer/README.md) exercises these
options through the unmodified `zstd` and `zstd-safe` Rust APIs.

### Consumer binding policies

`BindingOptions` supports per-name `macro_type_overrides`, a function blocklist,
caller-provided `raw_lines`, and `generate_cstr`. Name patterns are exact names or
prefixes ending in `*`; macro policies prefer exact names, then the longest prefix.
`CStr` generation rejects interior NUL bytes. Raw lines are appended verbatim and
are the caller's responsibility; reports list them separately from analyzed
declarations and record each deliberately blocked function.

The CLI exposes these as `--macro-type-for 'SQLITE_TRACE_*=unsigned'`,
`--blocklist-function NAME`, `--raw-lines-file FILE`, and `--generate-cstr`.
[SQLite consumer validation](../tools/sqlite_consumer/README.md) regenerates the
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
The [zstd fixture](../tools/zstd_consumer/README.md) validates both upstream and Toucan
bindings with Rust 1.64 and a pinned compatible dependency lock.

## Existing binding build scripts

The experimental [toucan_bindgen adapter](../crates/toucan_bindgen/README.md)
supports the builder calls used by the pinned zstd-sys and AWS-LC build scripts.
It selects Cargo's target and generates bindings during the build without
libclang. The adapter documents its supported arguments and API subset;
unsupported options produce errors. See [replacement readiness](replacement-readiness.md)
for the tested consumer paths and release blockers.

The proposed [uv and ty integration](opt-in-rollout.md) adds a build-time
`toucan-zstd` Cargo feature. The integration guide records complete builds of
both pinned applications and selected library and runtime checks without
libclang. The pinned upstream revisions do not expose this selector.

The `toucan_cli` package also provides a standalone `bindgen` executable. With
that executable first on `PATH`, the pinned `aws-lc-sys` build script selects it
when `AWS_LC_SYS_EXTERNAL_BINDGEN=1`. See the [external CLI guide](external-bindgen-cli.md)
for installation, arguments, and the native crypto and FIPS validation. The
unchanged `aws-lc-sys` build script rejects external mode with SSL enabled.
The original manifests still compile bindgen and clang-sys build dependencies.
The external FIPS build script also calls libclang and requires an upstream
change for a libclang-free installation.
