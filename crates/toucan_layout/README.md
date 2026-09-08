# toucan_layout

Toucan's C type layout engine, derived from `repc` and `repc-impl` 0.1.1 by
Julian Orth. The crate is self-contained and does not depend on a compiler or
`libclang` at runtime.

`compute_layout(target, ty)` preserves upstream defaults.
`compute_layout_with_compiler(target, compiler, ty)` also accepts Clang for the
x86-64 and AArch64 Linux GNU targets. It carries the compiler through nested
record, enum, typedef, and array layouts. Unsupported overrides return an error;
compiler flags and CPU features are separate from this choice.

The Windows target defaults continue to use the existing MSVC ABI engine.
Selecting Clang for a Linux target keeps Linux's physical ABI and platform rules.

## Provenance

The [upstream source](https://github.com/mahkoh/repr-c/tree/0c218ac5a6f82034e649fe749e7a902d7a43e8e0)
and both [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE) licenses are retained.
[UPSTREAM.md](UPSTREAM.md) records the import and local changes. Only the layout
library is included; the separate `cly` program and reference-test generator are
not dependencies.

## Validation

```console
cargo test -p toucan_layout
cargo test -p toucan_layout -- --include-ignored
```

The first command runs the 18 upstream unit tests, four documentation examples,
compiler-selection regressions, and exact comparisons against `repc` 0.1.1 for
every upstream target. The compiler oracle additionally requires GNU GCC and
Clang. On Linux it executes same-target record probes with each compiler; on all
hosts it cross-checks Clang's two Linux targets. `TOUCAN_GCC` can select an exact
GNU compiler executable.

`upstream_repc` is a test-only dependency for default-output comparison. Published
library users do not need it. The workspace depends on this package using a Cargo
package alias and an explicit version, so the selected engine remains part of
published dependencies.
