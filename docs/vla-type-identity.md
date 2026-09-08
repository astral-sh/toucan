# Variable-array type identities

`TypeKind::VariableArray` carries a `VariableArrayId` local to its analysis
result. Separately written runtime array types have distinct identities, even
when both bounds spell `n`. Reusing an object type through `typeof`, or declaring
two objects with one VLA typedef, preserves the identity. Separately written
prototype `[*]` bounds also differ.

These identities support exact deduced-type comparison. They do not change C
compatibility: two independently written VLA types can remain compatible. A
composite with two VLA types preserves the left type's identity; a composite with
one VLA and one incomplete array preserves the VLA identity. Neither choice
specifies when a bound runs. Checked `BoundId` and `TypeUse` metadata continue to
describe bound ownership and evaluation.

For example, Clang 18 rejects the first declaration group and accepts the second:

```c
void example(int n) {
    int a[n], b[n];
    __auto_type p = &a, q = &b;
}
void shared(int n) {
    typedef int A[n];
    A a, b;
    __auto_type p = &a, q = &b;
}
```

Clang compares the deduced types with `ASTContext::hasSameType` in
[`BuildDeclaratorGroup`](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaDecl.cpp#L14965).

The standalone identity layer records this distinction; support for Clang's
multiple and derived `__auto_type` declarations is a subsequent layer. The native
probe test checks Clang's rule separately from Toucan's current declaration
support.

## Construction and limits

A bounded AST prepass assigns ordinals before semantic checking. The registry
reserves candidates whose array bounds might be variable, including expressions
that later fold to constants. It uses source ranges to reconnect cloned helper
nodes to an already registered occurrence, never as the numeric identity.
Ambiguous or synthetic lookups produce diagnostics instead of choosing a match.
Ordinary and retained analysis use the same registry and construction order.

Expression evaluation fragments reserve a fresh range above the largest identity
in their inherited declaration types. Fragments without candidate arrays skip
that inherited-type traversal. A bare integer literal bound needs no registry
entry. The registry allocates only when a candidate exists.

The prepass and inherited-type walk share a four-million-step limit. At most
262,144 candidate occurrences are retained, and inherited type traversal is
limited to 128 levels. The traversal runs on the existing bounded parser stack;
parser input, AST, and work limits still apply. These limits report an error and
source position, rather than publishing a partial identity map.

The public Rust field and inspection schema migration are documented in
[inspection.md](inspection.md#migration-from-versions-1-and-2).

## Validation and cost

The [recorded evidence](../corpus/evidence/vla-type-identity-2026-09-08.json)
compares this layer with `f722e545effdc8fd734e1117b4160c10e99055bd`. Both builds
use release optimization and the system allocator. Semantic probes use matching
CLI dependency builds, count allocations, and run five alternating batches on
CPU 2. Binding generation uses seven alternating subprocess observations. These
are shared-host regression samples, not a comparison with a C compiler.

| Input | Normal analysis | Retained analysis | Binding generation |
| --- | ---: | ---: | ---: |
| zlib | +2.7% | +2.8% | +1.8% |
| sqlite | +1.5% | +1.6% | -0.1% |
| zstd | +1.2% | +1.3% | +2.9% |
| libgit2 | +2.8% | +2.3% | +0.4% |
| One VLA | +4.4% | +2.0% | — |
| 100 independent VLAs | +0.6% | +2.2% | — |

Positive percentages mean longer elapsed time. `TypeKind`, `Type`, and
`FunctionType` remain 32, 40, and 72 bytes on x86-64. Ordinary allocation fixtures
are unchanged; one VLA adds one 64-byte registry allocation. The 100-function VLA
fixture adds six registry allocations in normal analysis and 511 allocations
in retained analysis (1.8% over 28,385), which now keeps the independent array
types distinct.

All four binding outputs are byte-identical to the baseline. Complete declaration
output is equal between normal and retained analysis; removing the new identity
fields also gives the baseline output. Workspace tests, Clippy, Rustdoc, the
source-adapter regression, quota tests, native Clang probes, and 255 fixed-seed
checks across five targets passed.

The integrated layer at base `ece4c50cb67f26a55f721b38aebee0566f7f8226`
checks identity and query replay across all seven compiler/target profiles,
including Clang on both Linux targets. Inspection tests verify schemas 3/4 retain
both compiler metadata and VLA identities; atomic binding type keys accept the
added field. The timing table above remains the original measured baseline.
