# toucan_bindgen

A C binding builder for existing Rust build scripts, backed by Toucan's Rust
frontend. It does not require libclang or invoke a C compiler.

This adapter is experimental and implements a subset of bindgen's API. The pinned
zstd-sys 2.0.16 build script uses it unchanged: replace its build dependency with
the local package, keeping the dependency name `bindgen`:

```toml
[build-dependencies.bindgen]
package = "toucan_bindgen"
path = "../toucan/crates/toucan_bindgen"
features = ["runtime"]
default-features = false
```

The `runtime` feature accepts the existing manifest's feature selection. Both
configurations use Toucan's frontend. Building the adapter requires Rust 1.96;
generated declarations can target Rust 1.64 or later.

## Supported build scripts

`Builder` supports ordered `header` calls, `clang_arg`/`clang_args`, `use_core`,
`size_t_is_usize`, `rust_target`, `layout_tests`, `raw_line`,
`blocklist_function`, `blocklist_type`, and `rustified_enum(".*")`. Generation returns bindings
with `Display`, `write_to_file`, and a `report()` containing omitted macros and
dependencies. Compile-time size and alignment assertions remain enabled when
runtime layout tests are disabled.

Function blocklists accept exact identifiers or prefixes ending in `.*`, with
optional anchors. Other regular expressions and selective Rust enum conversion
produce errors. Type blocklists omit matching definitions and preserve their
uses under the names listed in `report().blocked_types`. Supply those Rust types
with imports or `raw_line`. The blocklist is not recursive: an independently
named dependency can still be emitted. Blocked anonymous enum typedefs also omit
their enum constants; a blocked alias of a named enum does not block that enum.
See [external types](../../docs/external-types.md) for layout and ABI contracts.
These boundaries are not general bindgen API compatibility.

The default representation uses `usize` for compatible `size_t`, unsigned types
for nonnegative integer macros, core paths, and Rust 1.64 syntax. Select enum
variants explicitly: Rust enums cannot represent arbitrary integer values.
Unsupported C syntax and unproved Rust calling ABIs remain generation errors.

## Targets and arguments

Cargo's `TARGET` selects the target. An explicit `--target` or `-target` overrides
it. Outside a build script, the supported native host is the default. Every target
uses the Clang semantic profile, including on Linux. Both GNU and musl Linux
triples are supported on x86-64 and AArch64; provide the selected libc
[sysroot and compiler resource headers](../../docs/musl.md).

Supported arguments are `-I`, `-D`, `-U`, `-isystem`, `-include`, `--sysroot`,
`-isysroot`, `-x c`, `-std=c11`, `-std=gnu11`, and trigraph overrides. Unknown options fail generation;
ABI-changing flags are never silently ignored. System include directories follow
ordinary include directories. A sysroot adds `usr/include` and the selected Linux
multiarch directory. Target headers and compiler resource directories must be
supplied explicitly; no host compiler or SDK discovery occurs.

The adapter defaults to GNU11. [C90, GNU90, C11, and GNU11](../../docs/language-modes.md)
select syntax, declaration rules, and predefined macros without enabling pedantic
diagnostics. C89 and GNU89 spellings are aliases for C90 and GNU90. Other language
modes remain explicit errors.

The adapter reads `BINDGEN_EXTRA_CLANG_ARGS` with shell quoting. Target-specific
forms take precedence, first using the Cargo target spelling, then replacing its
hyphens with underscores. Explicit builder arguments precede these environment
arguments. It captures one UTC timestamp per generation; `SOURCE_DATE_EPOCH`
selects a reproducible timestamp. Invalid values produce errors.

## Validation

`scripts/verify_bindgen_builder.py` compares four real zstd consumer feature
configurations against the original checked-in bindings. It changes only the
zstd-sys Cargo dependency, checks every upstream source hash, and requires rustc
dependency files to name the generated output. Compression, streaming,
dictionaries, experimental APIs, and shared thread pools must return identical
results and artifact bytes. The generated dependency graph must contain no
bindgen, clang-sys, or libloading packages. Native corpus CI runs this gate on
both Linux and macOS architectures.

The [saved Linux run](../../corpus/evidence/bindgen-builder-2026-09-08/summary.json)
records all four configurations and 25 byte-identical runtime artifacts. It
includes the consumed generated files and unchanged build-script hash.

For complete application paths, `scripts/verify_astral_builder.py` builds pinned
ty and uv through zstd-sys's unchanged build script and compares their compression
tests and CLI behavior. The [consumer guide](../../docs/astral-consumers.md#run-the-unchanged-bindgen-build-script)
describes the manifest-only activation and source/lock/artifact checks.
