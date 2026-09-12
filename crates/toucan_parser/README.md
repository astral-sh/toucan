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

### Reading and copying syntax

`parsed.ast()` borrows a node together with its arena. Struct fields have accessor
methods, enum variants are exposed by `kind()`, and child links resolve
automatically:

```rust
use toucan_parser::driver::{parse_expression, Config};
use toucan_parser::view::ExpressionView;

let parsed = parse_expression(&Config::with_gcc(), "a + b".into(), |_| false)?;
if let ExpressionView::BinaryOperator(binary) = parsed.ast().node().kind() {
    let lhs = binary.node().lhs();
    let independent = lhs.to_owned();
    assert!(lhs.structural_eq(independent.view()));
}
```

Views borrow their parse. `to_owned()` copies only the selected subtree and its
descendants into a fresh arena, so the copy can outlive the original. Cloning a
complete parse or owned `Ast` copies all its storage. `structural_eq()` follows
links in both owners and compares syntax and exact byte spans; allocation order,
unrelated records, source strings, and resource counters do not affect equality.

Compiler passes that consume or construct raw syntax can explicitly use
`into_raw()`. Existing `Visit` and `Printer` consumers can borrow `(node, arena)`
through `parsed.ast().as_raw()`. These low-level APIs require callers to keep IDs
with their arena: raw lookup checks bounds, and does not verify ownership. Raw
parts cannot be converted into an owner-bound view. Recursive raw AST types omit
`PartialEq`, and raw node cloning preserves IDs rather than copying descendants.

## Implementation

All parse entry points construct an owned typed arena. Recursive expression,
statement, initializer, and type/declarator links use IDs into per-type tables.
The returned `Parse` or `ExpressionParse` owns those tables alongside its root;
semantic analysis reads them directly. AST strings and nonrecursive lists retain
ordinary ownership and are released when the arena is dropped.

Four-byte IDs and the stored AST layout are shared by the owner-bound and raw
APIs. Views are borrowed handles and add no per-node storage. One exhaustive field
schema drives resource accounting, view accessors, structural equality, and
subtree copying. See the [arena benchmarks](../../benchmarks/arena/README.md) for
methodology.

The parser uses recursive descent for declarations and statements, and precedence
climbing for expressions. The lexer and parser live in [src/parser](src/parser).
Typedef names are tracked in lexical scopes to distinguish types from expressions.

## Upstream source

The AST, visitors, lexical environment, and reference tests derive from
[lang-c 0.15.1](https://github.com/vickenty/lang-c/tree/58e4ccf07bf9794af8111b06b6643e80fd31bff2)
by Vickenty Fesunov and contributors. Toucan replaces its generated parser with
handwritten code. The [original README](UPSTREAM.md), [MIT license](LICENSE-MIT),
and [Apache 2.0 license](LICENSE-APACHE) are retained.
