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

## Historical cost of adding limits to the generated parser

These measurements and the sanitizer replay below describe the earlier generated
parser. They do not measure the handwritten parser replacement.

The [measurement record](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/parser-budgets-2026-09-08.json) compares
an exact `bc889be` release build with this layer on Linux x86_64, using the system
allocator and CPU affinity 6. One warmup is excluded; each header has seven samples
and each source/mode pair has five. All four binding outputs and all seven complete
declaration outputs match, including normal/retained parity. Native `.i` routes
preserve their two accepted and five rejected cases; rejected runs are excluded
from these timing comparisons.

The additional checks increase runtime in these measurements. Full source analysis
remains a separate workload from binding generation.

| Header bindings | Baseline (ms) | With limits (ms) | Increase |
| --- | ---: | ---: | ---: |
| zlib | 40.4 | 45.9 | 13.6% |
| sqlite | 146.9 | 167.4 | 14.0% |
| zstd | 13.1 | 15.3 | 16.8% |
| libgit2 | 343.5 | 368.9 | 7.4% |

| Translation unit | Normal, baseline → limits (ms) | Retained, baseline → limits (ms) |
| --- | ---: | ---: |
| libgit2-alloc | 160.2 → 184.4 | 196.0 → 229.0 |
| libgit2-repository | 460.8 → 564.4 | 622.0 → 708.3 |
| sqlite-sqlite3 | 2011.3 → 2649.5 | 3716.4 → 4370.3 |
| zlib-adler32 | 55.2 → 63.7 | 64.5 → 73.4 |
| zlib-deflate | 80.1 → 100.3 | 111.9 → 133.5 |
| zstd-zstd_common | 96.2 → 119.2 | 122.0 → 143.1 |
| zstd-zstd_compress | 293.7 → 387.9 | 470.7 → 566.1 |

Normal source analysis increases by 15–32%; retained analysis increases by 14–20%.
The report includes peak RSS, every observation, output hashes, dependency hashes,
build defines/include paths, source hashes, and exact executable hashes. Process timings
include startup and complete output capture; source measurements also include full
declaration Debug serialization. These runs do not isolate every scheduling or
thermal effect.

The initial experiment that created one worker for every macro parse was rejected
because it added substantial binding-generation overhead. The final implementation
uses a scoped session for the complete binding operation.

## Sanitizer replay

The cached nightly (`rustc 1.100.0-nightly`, 2026-09-06) passed the parser resource
and session tests with AddressSanitizer, including the 16 MiB worker stack and
accepted AST clone/drop on a 2 MiB caller stack. A semantic fixed-seed replay
executed 18 inputs 16 times each (288 calls), with a 5-second per-input timeout
and 1,024 MiB RSS cap. It completed in 24.5 seconds without an address-sanitizer
finding or timeout; peak RSS was 409 MiB. This was seed replay, not mutational
fuzzing. Leak detection was disabled because LeakSanitizer could not use ptrace
in this environment; this evidence is not a leak check.

The record includes all seed contents, commands, toolchain version, binary/log
hashes, and sanitizer settings. To repeat the stack tests, use a nightly toolchain:

```console
RUSTFLAGS="-Zsanitizer=address -Cforce-frame-pointers=yes" cargo +nightly test -p toucan_parser --test resources --target x86_64-unknown-linux-gnu
```

Use `ASAN_OPTIONS=detect_leaks=0` only in an environment where LeakSanitizer is
unavailable, and record that limitation. The reproducible seven-TU source audit
and header benchmark commands are described in [the source audit](../corpus/translation-units.md)
and [benchmarking](../scripts/benchmark.py); the record supplies the exact input
requests and header arguments for both revisions.
