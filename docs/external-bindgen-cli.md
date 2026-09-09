# External `bindgen` executable

The `toucan_cli` package builds a second executable named `bindgen`. It accepts
the C-only argument sequence from the external-bindgen paths of `aws-lc-sys`
0.44.0 and `aws-lc-fips-sys` 0.14.2. It generates bindings through
`toucan_bindgen::Builder` without invoking libclang.

```sh
cargo build -p toucan_cli --bin bindgen --release
PATH="$(pwd)/target/release:$PATH" bindgen --version
```

The version output says `bindgen 0.0.1 (Toucan)`. The AWS-LC build script parses
the second word as a version, warns when it is below bindgen-cli 0.69.5, and
continues. The executable identifies its own package version rather than
claiming to be bindgen-cli 0.72.1.

The pinned `aws-lc-sys` 0.44.0 build script invokes this shape after
`AWS_LC_SYS_EXTERNAL_BINDGEN=1` is set and `bindgen` is on `PATH`:

```text
bindgen [--prefix-link-name aws_lc_0_44_0_] \
  --allowlist-file '.*(/|\\)openssl((/|\\)[^/\\]+)+\.h' \
  --allowlist-file '.*(/|\\)rust_wrapper\.h' \
  --rustified-enum point_conversion_form_t \
  --default-macro-constant-type signed \
  --with-derive-default --with-derive-partialeq --with-derive-eq \
  --raw-line '<AWS-LC copyright text>' \
  --generate functions,types,vars,methods,constructors,destructors \
  <aws-lc-sys>/include/rust_wrapper.h \
  --rust-target 1.70 --output <OUT_DIR>/bindings.rs --formatter rustfmt \
  -- -I <aws-lc-sys>/include -I <aws-lc-sys>/aws-lc/include
```

The generated `#[link_name]` uses the original C function or object name with
the requested prefix, except for the FIPS integrity symbol described below;
Rust names are unchanged. C has no methods, constructors,
or destructors, so these categories select nothing. `--` forwards the same
checked Clang-style include, macro, target, and language options supported by
the Builder adapter. Provide target C system headers and a sysroot explicitly;
the AWS-LC command alone supplies only AWS-LC's own include paths. For a native
Linux x86-64 environment with LLVM 18 headers, the paired consumer harness uses
`BINDGEN_EXTRA_CLANG_ARGS='-I /usr/lib/llvm-18/lib/clang/18/include --sysroot /'`.
Missing headers, unsupported arguments, and failed C
analysis return errors before writing the destination file.

This is a bounded executable interface, not general bindgen-cli compatibility.
Unsupported flags and category selections fail explicitly. The
[native Linux x86-64 AWS-LC check](../corpus/evidence/aws-lc-external-cli-317756d/README.md)
shows that the original upstream build script selects the executable, consumes
its output, and passes 41 crypto results, six C/Rust layouts, and 98 generated
layout tests. The same-command differential against bindgen-cli 0.72.1 shows
2,618 equal function signatures and 3,851 equal constants, plus 60 global
linker-name differences: Toucan prefixes the globals as requested, and the
actual AWS-LC archive contains all 60 prefixed definitions. A direct Rust link
test succeeds with Toucan and fails with bindgen-cli output for one of those
globals. Full API equality remains false. The separate
[native FIPS run](../corpus/evidence/aws-lc-external-fips-2026-09-09/README.md)
uses the unchanged `aws-lc-fips-sys` build script and tests an actual FIPS
integrity call, 41 matching crypto artifacts, six C/Rust layouts, and 97
generated layout tests. The FIPS symbol list deliberately leaves
`BORINGSSL_integrity_test` unprefixed: Toucan reads that list and emits the
correct linker name, while bindgen-cli incorrectly prefixes it. The unchanged
`aws-lc-sys` build script explicitly rejects external mode with `ssl` enabled.
Test each other target or feature selection separately. The unchanged
upstream manifest still compiles the bindgen Rust build dependency and its
`clang-sys`/`libloading` transitive crates in external mode. Its build script
also calls `bindgen::clang_version()` even when external generation is selected;
this unchanged upstream build does not establish a libclang-free installation.
