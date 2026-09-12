# Expression arena experiment

This standalone harness compares `parse_expression` with the draft
`parse_expression_arena` API. `arena-owned` also consumes the arena into the
existing owned AST, exposing the cost paid by existing consumers.

The prototype puts binary, assignment, conditional, and comma nodes in an arena.
Casts, unary/postfix expressions, and parenthesized subexpressions remain owned
leaves. Synthetic chains exercise arena nodes; zstd and zlib macro-body spellings,
with comments removed, expose the limitations of that coverage. These fixtures
compare parser structure without evaluating the macros. This harness does not measure
translation-unit parsing, semantic analysis, or complete binding generation.

Build two release executables with the same compiler and configuration:

```sh
cargo build --release --manifest-path benchmarks/arena/Cargo.toml
cp benchmarks/arena/target/release/toucan-arena-benchmark /tmp/arena-time
cargo build --release --manifest-path benchmarks/arena/Cargo.toml --features allocation-counting
cp benchmarks/arena/target/release/toucan-arena-benchmark /tmp/arena-alloc
python3 benchmarks/arena/run.py --timing /tmp/arena-time --allocations /tmp/arena-alloc --output benchmark-results/arena/results.json
```

Follow the local toolchain guidance when applicable (`cargo +ohm`, a distinct
`CARGO_TARGET_DIR`, and a separate shared `CARGO_BUILD_BUILD_DIR`). The runner pins
every child process to one allowed CPU, randomizes engine order within each pair,
and records raw samples, input/AST/binary hashes, host information, and compiler
configuration supplied by the caller. Avoid concurrent compilation while timing.

Each process checks equality of the owned AST and converted arena AST before
measurement, then warms its selected operation. Measurements run on one reused
frontend worker stack. Parse timing includes cloning the input string, parsing,
and (for `arena-owned`) conversion. Drop timing covers destruction of the result
and its source string. The reported total sums the two timings per iteration;
per-phase timer overhead is present in every engine.

Allocation accounting runs separately from timing. The feature wraps the system
allocator and counts allocations, reallocations, requested bytes (including full
reallocation sizes), and the peak increase in live requested bytes during one
parse-and-drop operation. It checks that live bytes return to their starting value.
It does not measure allocator metadata, stack usage, mapped pages, or process RSS.
The default timing build uses the uninstrumented system allocator.
