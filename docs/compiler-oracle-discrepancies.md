# Compiler oracle discrepancies

Compiler versions can differ in diagnostics and code generation. Toucan's
profiles select a compiler family and target, without identifying a particular
compiler version or optimization level. These records distinguish native oracle
behavior from the checked graph's source semantics.

## Apple Clang atomic-copy crash

Apple Clang 17.0.0 (`clang-1700.0.13.5`) crashed while syntax-checking the combined
atomic-copy fixture in the [Intel macOS run](https://github.com/astral-sh/toucan/actions/runs/34215630050/job/102026583137).
The driver reported a frontend segmentation fault. GCC 13 and upstream Clang 18
accept that source locally, and its runtime checks pass at O0 and O2.

The native oracle checks separate scalar, pointer, record, assignment, and inferred
initialization cases before the combined fixture. It collects all failures before
reporting them, so one compiler crash does not hide the other cases. A crash or
abnormal driver exit cannot satisfy an expected rejection. The Apple failure
remains a failing gate pending isolation; it is not recorded as C rejection or
successful conformance.

## Apple Clang SVE feature diagnostics

Apple Clang 17.0.0 (`clang-1700.0.13.5`) diagnoses an SVE value in the discarded
arm of `__builtin_choose_expr` without SVE enabled. Upstream Clang 18 accepts the
same type-only expression and emits no call. The
[Intel](https://github.com/astral-sh/toucan/actions/runs/34211061229/job/102011913138)
and [ARM](https://github.com/astral-sh/toucan/actions/runs/34211061229/job/102011913358)
macOS jobs exposed this diagnostic-phase difference when cross-compiling the
AArch64 fixture.

The oracle recognizes only that recorded Apple build and its specific missing-SVE
diagnostic. It then enables SVE for the compiler check and requires the assembly
to omit the discarded function call. Other compiler errors still fail the test.
A scalar-call control checks that the assembly matcher recognizes the target's
symbol spelling. Toucan's feature-use checks retain their evaluated-use policy;
this does not establish an SVE execution ABI on Darwin.

## Clang 18 conditional VLA bounds

```c
int token;
int f(int condition) {
    int n = 3;
    void *p = condition ? (void *)&token : (int (*)[n++])0;
    return n;
}
```

C11 requires `f(0)` to return 4 and `f(1)` to return 3. GCC 13.3 produces those
values. Clang 18.1.3 produces 4 for both calls at `-O0` and `-O2`. Replacing the
condition with the literal `1` suppresses the increment in both compilers.
Adding an ordinary side effect to an arm also prevents the observed Clang
speculation. No `__auto_type` extension is involved.

[C11 N1570](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf)
6.5.15 paragraphs 4 and 6 require selected-arm evaluation and give this
expression type `void *`. No composite array type is formed in this example. The address of `token` is
used deliberately: `(void *)0` would be a null-pointer constant and would give
the conditional the other operand's VLA pointer type.
The bound is attached to the cast in the third operand; its evaluation remains
subject to that operand's selection. The retained graph keeps that cast, its
`TypeUseId` and runtime `BoundId` even though conversion to `void *` removes the
extent from the conditional's result type. Enclosing `sizeof` on this pointer
expression and `_Generic` suppress the bound entirely.

The observed implementation cause is Clang 18's
[`isCheapEnoughToEvaluateUnconditionally`](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/CodeGen/CGExprScalar.cpp#L4824):
it uses constant evaluability to choose an LLVM select and evaluates both arms.
That test overlooks the cast's runtime type-bound effects. Toucan does not add
that speculation to its checked IR.

Probes that combine two freshly written VLA pointer types need separate care.
C11 6.2.7 paragraph 3 makes formation of a composite type from an unevaluated
runtime bound undefined in the specified case; 6.7.6.2 paragraph 6 also requires
matching runtime sizes when compatibility is required. Merely starting two
counters at equal values does not resolve the unevaluated-bound problem.
`BoundValue::Composite.selection` records the source condition; it does not
promise a runtime selection rule for the composite extent.

[Saved evidence](../corpus/evidence/conditional-vla-oracles-2026-09-08.json)
contains the exact sources, commands, compiler versions, equal and unequal
bound probes, fixed-size and void-pointer cases, and primary-source hashes.
The regression tests check retained source structure and unevaluated contexts;
they do not require future Clang versions to reproduce this bug.
