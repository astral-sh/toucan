# Architecture

The frontend separates parsing and target semantics from binding-output policy.

| Component | Responsibility |
| --- | --- |
| `toucan_preprocessor` | Includes, tokens, macros, conditional expressions, provenance, and preprocessing limits. |
| `toucan_parser` | Handwritten recursive-descent declarations/statements and precedence-climbing expressions; AST and lexical scopes derived from lang-c. |
| `toucan_semantic` | Declaration identity, C types, constant evaluation, body checking, and optional retained checked code. |
| `toucan_layout`, `toucan_target` | Attributed repc layout fork, physical data models, compiler profiles, and target macros. |
| `toucan_bindings` | Selection, Rust representations, call-ABI restrictions, layout assertions, and bitfield accessors. |
| `toucan` | Integrated configuration, preprocessing/analysis ownership, macro evaluation, and reports. |
| `toucan_bindgen` | Supported bindgen Builder API, callbacks, selection policies, and optional rustfmt. |
| `toucan_cli` | Command-line configuration, file output, diagnostics, and optional process allocator. |
| `toucan_stack` | Shared scoped worker stacks for recursive frontend operations. |

## Target and compiler identity

`CompilerProfile` combines a physical target, compiler family, and language mode.
It is independent of the host running Toucan and is retained with the translation
unit. Later constant and layout queries use the same profile. Include paths and
sysroots are explicit inputs; compiler version macros select supported header
branches rather than asserting implementation of an entire compiler release.
See [compiler profiles](compiler-profiles.md).

## Semantic ownership

The preprocessor returns expanded text and mappings to original file locations.
The integrated library attaches those locations to semantic diagnostics. Macro
locations identify the outer invocation, without a full expansion backtrace.

The semantic layer checks the whole input, including function bodies and
initializers, before binding selection. An allowlist cannot hide unsupported C
syntax from analysis. Integers retain width, signedness, and rank; floating
constant evaluation uses target-format software arithmetic. Storage layout,
type identity, and function-call representation are separate concerns.

`Analysis` owns declarations and optional metadata. `Compilation` additionally
owns preprocessed text and provenance. Both expose immutable views; consuming
`Analysis::into_unit` discards retained code before returning mutable declaration
data. IDs belong to one owner and cannot be mixed between analyses.

Retaining checked code is optional. The graph contains typed expressions,
conversions, lexical declarations, statements, initializer structure, and runtime
array bounds. It records source structure rather than a lowered control-flow
graph or an execution order for C side effects. See the [analysis API](analysis-api.md).

## Rust representations

Ordinary records use their actual fields with `repr(C)`. Generated assertions
check target identity, size, alignment, and ordinary field offsets. Function
pointers use nullable `Option<unsafe extern ... fn(...)>` representations where
appropriate. Padding and partially initialized storage use `MaybeUninit`.

Matching size and alignment does not prove matching argument registers, return
conventions, or Rust value validity. The binding emitter separately rejects
unsupported call representations, including selected bitfield, flexible-array,
atomic aggregate, and target-specific over-aligned cases. Transparent unions
require a validated parameter carrier. See [compatibility](compatibility.md).

## Process and resource policy

Library source forbids unsafe Rust; generated FFI declarations expose unsafe
operations. The integrated frontend does not invoke a C compiler. The parser's
standalone compatibility driver can launch a configured preprocessor, and the
Builder can run rustfmt when writing or displaying output. Only the CLI selects
a process allocator. See [library usage](library.md).

Preprocessing, parsing, semantic nesting, and retained graphs have explicit
resource limits. Related recursive operations share a scoped 16 MiB worker stack;
nested operations reuse it. A worker is joined before the public call returns.
Filesystem access can be disabled for in-memory inputs. These counters do not
constitute a universal wall-time or resident-memory bound. See [parser limits](parser-limits.md).
