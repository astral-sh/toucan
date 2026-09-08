# Compiler oracle discrepancies

Native compilers provide useful differential evidence, but an optimization bug
is not a C language rule. Toucan's target profiles do not identify a particular
compiler version or optimization level. The checked graph preserves source
semantics when a native result contradicts C11.

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
