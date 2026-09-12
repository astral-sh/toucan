# Parser resource limits

The handwritten C parser counts work while it tokenizes and parses.
It stops with a source-positioned resource diagnostic when a limit is reached.
Optional rules and lookahead cannot turn resource exhaustion into a successful parse.

## Limits

`toucan_parser::driver::parse_preprocessed` uses these defaults:

| Resource | Default |
| --- | ---: |
| Preprocessed input | 16 MiB |
| Total accounted work | 2,000,000,000 units |
| Rule/loop steps without advancing the furthest examined byte | 1,000,000 |
| Active recursive parsing calls, including precedence parsing | 512 |
| Owned AST depth, including container wrappers | 1,024 |
| Retained token-buffer capacity | 256 MiB |
| Live construction-metric entries | 500,000 |

`parse_preprocessed_with_limits` accepts a `ParseLimits` value. Rule-depth limits
above 512 and AST-depth limits above 1,024 are rejected before starting a worker;
these ceilings protect recursive stack frames and owned-tree cleanup. The remaining
limits can be raised or lowered. Each parse has independent counters and lexical
state. Successful parses and errors expose `ParseStatistics`; parser errors also
expose the resource kind, limit, observed count, and preprocessed byte offset.

Total work includes input bytes, token storage, recursive entries, repetition and
precedence steps, constructed node bytes, structural visits, and construction-map
cleanup. The lexer charges the input scan before examining bytes and checks buffer
capacity before allocation. `max_cache_bytes` bounds that capacity;
`ParseStatistics::cloned_bytes` is zero because parsing does not clone ASTs. The separate backtracking counter resets only when the parser examines a
new furthest byte. Padding a pathological expression with whitespace therefore does
not increase its backtracking allowance. These are deterministic accounting units
for a given parser build, not a wall-time deadline or an allocator RSS measurement.

The experimental `parse_expression_arena_with_limits` entry point uses the same
limits. Arena nodes and cached construction measurements share the metadata-entry
quota, and arena capacity growth is charged to work before allocation. AST depth
measures the equivalent owned tree, including retained owned leaves, so explicit
`into_owned()` conversion preserves the cleanup depth bound. Conversion itself is
outside parser work accounting and allocates compatibility boxes and temporary
storage. The arena representation currently covers outer binary, assignment,
conditional, and comma operators; it does not replace translation-unit parsing.

C11 minimum nesting is tested explicitly: 63 levels of parenthesized expressions
and declarators, 127 blocks, 63 record definitions, and 12 derived pointer modifiers.
Conditional-inclusion depth belongs to the preprocessor. Flat bodies containing more
than 1,024 control-flow tokens are accepted; nesting is measured from the parser's
actual recursion and owned AST, rather than a scan that conflates siblings with
ancestors.

The upstream `driver::parse` convenience entry point invokes an external C
preprocessor and applies these guards to its resulting text. Toucan uses its own
preprocessor and `parse_preprocessed`; it launches no compiler. External compiler
execution and output collection in that upstream convenience path are outside the
parser's work accounting.

## Stack and owned trees

Parsing runs on a scoped 16 MiB worker stack. Worker-creation errors become resource
diagnostics. The worker is joined before return, with no idle background thread.
`with_parser_stack` groups related operations on one stack, and nested sessions
reuse it. Preprocessing, semantic analysis/evaluation, and final-environment macro
queries share the same worker through `toucan_stack`. Binding generation groups
all of its macro parses in one session to avoid repeated thread creation.

Preprocessor include and macro expansion limits cannot exceed 256. Their defaults
remain 64 and 128. The ceilings also apply when preprocessing files, ordered file
lists, or in-memory headers directly. Unsupported settings fail before processing
input. `with_preprocessor_stack` lets standalone consumers group repeated calls
and macro queries into one session. The limits cover frontend recursion; caller
feature-query providers and closures must bound their own work and recursion.

Every node constructor and each binary/postfix fold checks the resulting
owned subtree before it can become another fold's child.
Private measurements cache recursive ownership boundaries by payload kind and exact
source span. Repeated identities retain conservative maximum depth and clone size.
Constructors always refresh their root, including the declaration-extension mutator.
Completed external declarations release child measurements. An exhaustive structural
schema names every AST field and variant; the upstream reference tests compare
cached measurements against an uncached walk at each constructor.

Semantic type construction separately checks each
derived modifier, since a flat `********p` parser declarator creates a nested owned
semantic type. The parser reads `__int128` and empty initializers directly, so AST
spans retain the original preprocessed offsets. Native line markers retain the
existing source-remapping path for diagnostics.

Anonymous record member validation allows one million field visits per containing
record. Microsoft anonymous tag and typedef members can share nested record
definitions, so this work bound also counts repeated visits through that graph.
Exceptionally expensive valid member graphs return a resource diagnostic.

Tests run hostile unary, postfix, binary, declarator, label/control, conditional,
and malformed-prefix inputs in separate worker processes. They also clone and drop
accepted deep ASTs on a 2 MiB caller stack, sweep work failures across backtracking
and optional-rule paths, and check concurrent calls, session reuse, and panic
propagation. Parser limits do not constrain arbitrary user callbacks, downstream
visitors, or manually constructed ASTs.

## Sanitizer checks

Run the stack and resource tests with a nightly toolchain:

```console
RUSTFLAGS="-Zsanitizer=address -Cforce-frame-pointers=yes" cargo +nightly test -p toucan_parser --test resources --target x86_64-unknown-linux-gnu
```

Use `ASAN_OPTIONS=detect_leaks=0` only where LeakSanitizer is unavailable, and
record that limitation. See [fuzzing](../fuzz/README.md) for mutation campaigns,
[the source audit](../corpus/translation-units.md) for complete translation units,
and [benchmarking](benchmarks.md) for performance measurements.

The [historical limits report](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/parser-budgets-2026-09-08.json)
measures the earlier generated parser; it does not measure the handwritten parser.
