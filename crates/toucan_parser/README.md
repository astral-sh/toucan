# toucan_parser

The C parser used by Toucan, derived from [lang-c 0.15.1](https://github.com/vickenty/lang-c/tree/58e4ccf07bf9794af8111b06b6643e80fd31bff2).
Toucan uses `driver::parse_preprocessed`; preprocessing, semantic checking, source limits, and checked-code retention live in the other workspace crates. This crate retains the upstream parser AST and is not the public checked-code interface.

## Upstream source

- Author: Vickenty Fesunov <kent@setattr.net>, with upstream contributors.
- Git revision: `58e4ccf07bf9794af8111b06b6643e80fd31bff2` (tag `0.15.1`).
- Published `lang-c` 0.15.1 crate SHA-256: `720e6492b795d1f6838eb2e51879ec3073be745a20e52c32a241b80b3c8ed998`.
- Licenses: [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), copied from upstream.
- [Original README](UPSTREAM.md), grammar, generated parser, and all 139 reference cases are retained.

The workspace publishes this fork as `toucan_parser`; `toucan_semantic` depends on that package using the `lang-c` alias. Packaged builds therefore use the same parser as workspace builds.

## Changes from upstream

`typeof_specifier0` tries `type_name` before `expression0`. A visible typedef such as `T` must parse as a type in `__typeof__(T)`, `__typeof__(T *)`, `__typeof__(T[3])`, and `__typeof__(T (void))`. The lexical environment selects the expression branch when an ordinary identifier shadows a typedef. Definition parsing now retains the selected parameter scope through the body, including parameter names and enumerators; callback parameter scopes and return-type prototypes remain separate. Ordinary function declarations continue to discard their prototype scopes. Parenthesized expressions retain their original AST and byte spans; the grammar does not reinterpret arbitrary expressions as types.

`__extension__` accepts a cast expression as its operand, preserving the operand's type and value. This includes the `__extension__ (int)1` spelling used in compiler resource headers.

The package name and imports in examples and development binaries are updated. Handwritten code has mechanical fixes for current Rust and Clippy warnings. Two local lint allowances preserve the existing AST representation and `Span::span` API. The generated header enumerates the Clippy style lints produced by the pinned generator. The crate forbids unsafe Rust. No generator is run during ordinary builds.

## Regeneration

Install the upstream generator in a separate tooling directory:

```console
cargo install peg --version 0.5.4 --root /tmp/toucan-parser-tools
make -B src/parser.rs peg=/tmp/toucan-parser-tools/bin/rust-peg
cargo fmt -p toucan_parser --check
cargo test -p toucan_parser
```

Run `make` from this directory. The checked-in parser was generated with `peg` 0.5.4 and formatted with `rustfmt` 1.9.0-stable (Rust 1.98.0); `grammar.rustfmt` fixes the output settings. Compared with the upstream parser, regeneration changes only `typeof_specifier0`, the function-declarator scope rules, the `__extension__` operand rule, and the documented lint header. Review that diff when regenerating with another formatter version.

The upstream reference runner reads `reftests/`. It updates expected output only when `TEST_UPDATE` is explicitly set. New parser tests cover typedef/type-expression ambiguity; semantic tests compare constraints and runtime VLA behavior with GCC and Clang.
