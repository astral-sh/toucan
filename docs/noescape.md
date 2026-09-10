# `noescape` parameter contracts

Clang profiles recognize GNU-spelled `__attribute__((noescape))` on pointer
parameters, including adjusted array and function parameters. GCC profiles ignore
this unknown attribute, as GCC 13 does. Clang ignores nonpointer and nonparameter
subjects; argument-count errors apply to parameter subjects. Diagnostics for
ignored attributes are not currently emitted as warnings.

A contract promises that references derived from the parameter pointer do not
survive the call. Freeing the pointer is permitted. Toucan retains that promise;
it does not prove that the definition obeys it. A definition that stores its
parameter globally is still accepted by the pinned Clang oracle and Toucan.

## Types and declarations

`FunctionType::parameter_contracts` indexes
`TranslationUnit::parameter_contracts`. Each nonempty set contains sorted,
zero-based `no_escape` parameter positions. Source-created sets are deduplicated.
The ID belongs to its owning translation unit. Public caller-built units are
validated before constant queries and binding generation, including nested
function types, parameter positions, and pointer subjects.

Contracts participate in exact type identity, including typedef redeclarations.
They do not make C function types incompatible. Compatible prototype
redeclarations and conditional pointer types intersect their contracts. A block
declaration merges with a visible declaration; declarations in exited blocks do
not retroactively change the contract visible elsewhere.

Clang 18's assignment rule rejects a strict strengthening when its reverse would
be an otherwise exact function conversion. This applies to function pointers and
one additional pointer level. Dropping promises is accepted. Crossing masks,
three-level pointers, and differences inside callback parameter types remain
C-compatible. Explicit casts are accepted. These distinctions follow Clang's
source rules; they are not a general contract-subtyping policy.

## Retained analysis

`CheckedCode::noescape_attributes()` preserves written source ranges and their
owning occurrences, including duplicates and ignored subjects. Its parameter
applications identify the written declaration sites and whether Clang applies
the annotation to the adjusted parameter type. This is separate from the
**effective** contract after redeclaration merging.

Read effective call contracts through each retained callee's effective type.
Read effective definition-entry contracts through `FunctionBody::signature()`.
An identifier-list definition can carry entry promises in that body signature
while its public callable type remains nonprototype and has no fixed-parameter
contract. Earlier calls retain their declaration-time types after later
redeclarations remove a promise.

## Bindings and limits

`noescape` does not change the physical calling convention. Generated function
and callback signatures include a comment identifying each promised parameter.
Rust function-pointer types cannot enforce this promise; callback implementers
must uphold it when C calls them through the annotated type. No escape-analysis
or additional Rust ABI guarantee is claimed.

The sparse arena is limited to 65,536 contract sets and 1,048,576 parameter
positions. Retained arena rows and written annotations are charged to the existing
node, edge, and payload budgets before their owned allocation. Function and
parameter sizes remain unchanged on the measured x86-64 host. The optional arena
and interner do not allocate for ordinary unannotated functions.

Native tests check source constraints on all five Clang targets and native GCC,
LLVM `nocapture` on declarations and declaration-time calls, and actual C/Rust
callbacks at multiple optimization levels. The retained tests cover written and
effective contracts, nontrivial intersections, lexical visibility, K&R entries,
and malformed caller-built IDs. Evidence distinguishes cross-target compilation
from native execution.

Primary references: [Clang 18 attribute documentation](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/include/clang/Basic/AttrDocs.td),
[parameter-info merging](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/AST/ASTContext.cpp),
[function conversions](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/Sema/SemaOverload.cpp),
and [C pointer assignment](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/Sema/SemaExpr.cpp).

The local 42-case allocation comparison uses three ordinary inputs on all seven
profiles, both direct calls and a reused parser session. Allocation counts match
exactly. Reused-session allocated bytes also match exactly; direct calls allocate
48 additional bytes in the worker packet, independent of declaration count.
Five alternating timing rounds on four real preprocessed headers produced median
current/base ratios from 1.005 to 1.013 for the final layer on a shared host. The
initial noescape-only batch is also preserved (0.995 to 1.054). These observations
do not establish a speedup or a stable throughput regression.

### Interaction with GNU `noreturn`

Clang's GNU-spelled `noreturn` is a function-type promise, retained in
`FunctionType::noreturn`. C11 `_Noreturn` remains declaration semantics and does
not set this type bit. Exact typedef identity compares the bit; C compatibility
ignores it. Declaration composites preserve it when either declaration has it,
while conditional composites preserve it only when both operands have it.

Clang's function conversion can drop both kinds of promise. The reverse-conversion
check therefore considers `noreturn` and `noescape` together; crossing promises
can be accepted even though either independent strengthening would be rejected.
The differential tests cover all sixteen combined promise pairs. GCC's ignored
bare-function typedef attribute does not acquire a Clang type bit.

Rust signatures retain the written C return type and carry a `noreturn` comment
for callback implementers. The generator does not substitute Rust's never type
or claim that its function-pointer type enforces the C promise.

The [frozen-layer evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/function-contracts-2026-09-08.json)
records compiler decisions, LLVM contracts, callbacks, allocation observations,
and untouched zstd translation units. The [integration checks](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/function-contracts-integration-2026-09-08/summary.json)
cover the newer language-mode and BMI layers, including 1,064 ordinary/retained
seed cases and native callbacks with both the current Rust toolchain and Rust 1.64.
