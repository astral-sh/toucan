# toucan_layout

Computes C type sizes, alignments, and field offsets for a selected target without
invoking a compiler or loading `libclang`.

`compute_layout(target, ty)` uses the target's default compiler rules.
`compute_layout_with_compiler(target, compiler, ty)` selects compiler rules
explicitly and rejects unsupported combinations. Both accept a type tree with
records, bitfields, arrays, enums, typedefs, and packing or alignment annotations.
Sizes, offsets, and alignments are measured in bits.

See the [crate documentation](src/lib.rs) for an example. Toucan uses this engine
through [`toucan_target`](../toucan_target), which adds target profiles and
serializable input and output types.

## Provenance

Derived from `repc` and `repc-impl` 0.1.1 by Julian Orth. The
[upstream source](https://github.com/mahkoh/repr-c/tree/0c218ac5a6f82034e649fe749e7a902d7a43e8e0)
is licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE).
[UPSTREAM.md](UPSTREAM.md) records the import and local changes. The separate
`cly` program and reference-test generator are not included.

## Validation

```console
cargo test -p toucan_layout
cargo test -p toucan_layout -- --include-ignored
```

The ignored compiler comparisons require GCC and Clang. Set `TOUCAN_GCC` to
select a GNU compiler executable.
