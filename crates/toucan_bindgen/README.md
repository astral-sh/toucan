# toucan_bindgen

A C binding generator with a subset of bindgen's build-script API. It uses
Toucan's Rust frontend and does not require libclang or invoke a C compiler.

The adapter is experimental. It supports the calls used by the pinned zstd-sys
and AWS-LC consumers; other build scripts may need changes. Unsupported frontend
arguments and binding representations return errors.

## Use in a build script

Replace the `bindgen` build dependency with this package, keeping the dependency
name. Adjust the path to your Toucan checkout:

```toml
[build-dependencies.bindgen]
package = "toucan_bindgen"
path = "../toucan/crates/toucan_bindgen"
features = ["runtime"]
default-features = false
```

The `runtime` feature is accepted for compatibility and has no effect. Building
the adapter requires Rust 1.96 or later.

For a header named `wrapper.h`, add this to `build.rs`:

```rust
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .rust_target("1.85".parse()?)
        .generate()?;

    for header in &bindings.report().dependencies {
        println!("cargo:rerun-if-changed={}", header.display());
    }
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    bindings.write_to_file(out_dir.join("bindings.rs"))?;
    Ok(())
}
```

Include the output with `include!(concat!(env!("OUT_DIR"), "/bindings.rs"));`.
The example targets Rust 1.85 so its declarations work with edition 2024.
The default output targets Rust 1.64 for consumers using older editions. The
application must still build or link the C library it calls.

## Configuration

Cargo's `TARGET` selects the target unless a `clang_arg("--target=...")` overrides
it. Supply target headers through `clang_arg` or `clang_args`, using `-I`,
`-isystem`, or `--sysroot` as needed. The adapter does not discover system SDKs or
compiler installations. It also reads `BINDGEN_EXTRA_CLANG_ARGS` and its
target-specific variants.

## Further reading

- [Builder API](src/lib.rs): supported methods and defaults.
- [Header setup](../../docs/usage.md#system-headers-and-cross-compilation) and
  [target coverage](../../docs/compatibility.md#targets).
- [Consumer integration](../../docs/astral-consumers.md#run-the-unchanged-bindgen-build-script):
  use the adapter through zstd-sys in uv and ty.
- [Replacement readiness](../../docs/replacement-readiness.md): tested consumers
  and remaining compatibility work.
