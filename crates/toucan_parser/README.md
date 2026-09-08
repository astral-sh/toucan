# toucan_parser

The C parser used by Toucan, derived from [lang-c 0.15.1](https://github.com/vickenty/lang-c/tree/58e4ccf07bf9794af8111b06b6643e80fd31bff2).
Toucan uses `driver::parse_preprocessed`; preprocessing, semantic checking, and checked-code retention live in the other workspace crates. The parser enforces its own input, work, backtracking, recursion, owned-tree, and memoization limits. This crate retains the upstream parser AST and is not the public checked-code interface.

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

`_Thread_local` and GNU `__thread` retain distinct storage-specifier variants,
allowing semantic checking to apply their different ordering rules.

`__bf16` has a distinct type-specifier node; it is not inserted as an integer
typedef. The existing TS 18661 grammar retains `_Float16` and `f16` literals.

GNU attributes on null statements have a distinct `Statement::Attribute` node.
Semantic checking determines which statement annotations are supported; the visitor
preserves their attributes and source spans.

`__builtin_types_compatible_p` and `__builtin_choose_expr` have dedicated AST nodes. Their type-name and expression operands retain original spans; the checked frontend supplies type compatibility, selected value categories, and evaluation contexts.

Parsing uses a bounded scoped worker stack; `driver::with_parser_stack` reuses one
worker for a batch of calls. `driver::parse_preprocessed_with_limits` accepts limits
and returns source-positioned resource diagnostics. `ParseStatistics` records actual
accounted work. The AST representation is unchanged. See
[parser limits](../../docs/parser-limits.md) for the accounting and tests.

The package name and imports in examples and development binaries are updated. Handwritten code has mechanical fixes for current Rust and Clippy warnings. Two local lint allowances preserve the existing AST representation and `Span::span` API. The generated header enumerates the Clippy style lints produced by the pinned generator. The crate forbids unsafe Rust. No generator is run during ordinary builds.

## Regeneration

Install the upstream generator in a separate tooling directory:

```console
cargo install peg --version 0.5.4 --root /tmp/toucan-parser-tools
make -B src/parser.rs peg=/tmp/toucan-parser-tools/bin/rust-peg
cargo fmt -p toucan_parser --check
cargo test -p toucan_parser
```

Run `make` from this directory. `scripts/instrument.py` checks and instruments the pinned generated templates after
formatting. It inserts rule/loop guards, terminal resource-failure propagation, and
memoized clone accounting. The generated lint header permits the immediate closures
and explicit returns required to balance recursive-rule counters on early exits.
A normal Cargo build does not invoke Python. The checked-in parser was generated with `peg` 0.5.4 and formatted with `rustfmt` 1.9.0-stable (Rust 1.98.0); `grammar.rustfmt` fixes the output settings. Compared with the upstream parser, regeneration changes only `typeof_specifier0`, the function-declarator scope rules, the `__extension__` operand rule, GNU attribute statements, GNU thread storage, bfloat type syntax, type introspection, delayed-scope `__auto_type` declarations, GNU real/imaginary unary operators, checked node/fold constructors, resource instrumentation,
and the documented lint header. Review that diff when regenerating with another formatter version.

The upstream reference runner reads `reftests/`. It updates expected output only when `TEST_UPDATE` is explicitly set. New parser tests cover typedef/type-expression ambiguity; semantic tests compare constraints and runtime VLA behavior with GCC and Clang.
