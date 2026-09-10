# toucan_parser

A C parser for Toucan, written in Rust. It parses C90 through C17 syntax with
GNU, Clang, and Microsoft extensions into an abstract syntax tree (AST).

## Usage

`driver::parse_preprocessed` accepts preprocessed C source and returns an AST
with byte spans. `driver::Config` selects the language version and extensions.
For preprocessing and type checking, use the
[`toucan` library](../../docs/library.md).

`driver::parse_preprocessed_with_limits` accepts limits for input size, parsing
work, recursion, AST depth, and token storage. See [parser limits](../../docs/parser-limits.md)
for configuration and accounting.

## Implementation

The parser uses recursive descent for declarations and statements, and precedence
climbing for expressions. The lexer and parser live in [src/parser](src/parser).
Typedef names are tracked in lexical scopes to distinguish types from expressions.

## Upstream source

The AST, visitors, lexical environment, and reference tests derive from
[lang-c 0.15.1](https://github.com/vickenty/lang-c/tree/58e4ccf07bf9794af8111b06b6643e80fd31bff2)
by Vickenty Fesunov and contributors. Toucan replaces its generated parser with
handwritten code. The [original README](UPSTREAM.md), [MIT license](LICENSE-MIT),
and [Apache 2.0 license](LICENSE-APACHE) are retained.
