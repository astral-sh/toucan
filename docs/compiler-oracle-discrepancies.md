# Compiler oracle discrepancies

Compiler versions can differ in diagnostics and code generation. Toucan's
profiles select a compiler family and target, without identifying a particular
compiler version or optimization level. These records distinguish native oracle
behavior from the checked graph's source semantics.

## Apple Clang atomic-copy crash

Apple Clang 17.0.0 (`clang-1700.0.13.5`) crashes on this pointer-to-atomic-pointer cast:

```c
void f(int n) {
    _Atomic(int *) p = (_Atomic(int *))&n;
}
```

The [isolated Intel macOS run](https://github.com/astral-sh/toucan/actions/runs/34216957132/job/102030873020)
reproduced the crash in initialization, assignment, inferred initialization, and
both runtime optimization levels. Scalar and record copies passed. The driver
returns exit code 1 with a frontend segmentation-fault diagnostic; this is a
compiler failure, not source rejection. Upstream Clang 17 and 18 accept the exact
combined source for Linux and both Darwin targets, compile its runtime fixture,
and execute it successfully on native x86-64 Linux.

For this recorded Apple build, the atomic-copy fixture uses upstream Clang 18 on
the same host for the exact crashing cast cases. Set
`TOUCAN_ATOMIC_POINTER_CLANG` to that compiler's path when running the native tests
locally; macOS CI installs `llvm@18` and sets it explicitly. The fixture checks the
replacement's version. Missing compilers, crashes, unexpected diagnostics, and
runtime failures still fail the test. Other Apple versions use the original oracle.

Apple Clang continues to check the remaining cases, including atomic pointer
copies from function results at O0 and O2. The cast cases do not establish
compatibility with the affected Apple compiler. The
[saved report](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/apple-atomic-pointer-oracle-2026-09-08.json)
preserves the failing job, exact sources, flags, and upstream probe results.
The [ARM macOS validation](https://github.com/astral-sh/toucan/actions/runs/34219192884/job/102038043364)
passed the full native workspace suite with Apple Clang and Homebrew Clang 18.1.8
under this selection policy.

## Apple Clang invalid `va_arg` crash

Apple Clang 17.0.0 (`clang-1700.0.13.5`) diagnoses a function type passed to
`__builtin_va_arg`, then crashes during object generation when targeting
x86-64 Linux. The [Intel macOS job](https://github.com/astral-sh/toucan/actions/runs/34219192884/job/102038043391)
returned exit code 1 with an illegal-instruction diagnostic. The shared oracle
classifier correctly treats this as a compiler failure.

We check this one invalid Clang source with `-fsyntax-only`. Valid function-pointer
`va_arg` calls still generate objects, as do all other positive and negative
cases. In particular, GCC defers some invalid `va_start` diagnostics until body
lowering, so syntax-only checking cannot replace those object-generation probes.
The [saved report](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/va-arg-oracle-phases-2026-09-08.json)
records the failure and successful upstream Clang and GCC diagnostic probes.
The [Intel](https://github.com/astral-sh/toucan/actions/runs/34222265563/job/102047928026)
and [ARM](https://github.com/astral-sh/toucan/actions/runs/34222265563/job/102047928197)
macOS jobs passed all 706 workspace tests each, including this fixture and its
valid function-pointer control. Both Linux architectures passed 708 tests each.
The [native validation archive](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/native-2baa121/summary.json)
preserves the logs and verifies that the tested merge tree equals `2baa121`.
These results cover that commit; later language changes require fresh validation.

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

[Saved evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/conditional-vla-oracles-2026-09-08.json)
contains the exact sources, commands, compiler versions, equal and unequal
bound probes, fixed-size and void-pointer cases, and primary-source hashes.
The regression tests check retained source structure and unevaluated contexts;
they do not require future Clang versions to reproduce this bug.

## GCC 14 old-style parameter bounds

```c
int events;
int bound(int x) { events = events * 10 + x; return 2; }
int shape(a, b)
    int (*b)[bound(2)];
    int (*a)[bound(1)];
{
    return events;
}
```

With zero-initialized `events`, GCC 13.3 and Clang 18 return 12. Ubuntu GCC 14.2
(`14.2.0-4ubuntu2~24.04.1`) returns 0 at both O0 and O2. Homebrew GCC 14.4.0
returns 0 on both native macOS architectures. The parameters remain pointers to
variable-length arrays after adjustment, so their bounds are not discarded by
array-to-pointer parameter adjustment. C11 [N1570](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf)
6.9.1 paragraph 10 requires their entry effects. Either parameter evaluation order
is accepted by the test.

The equivalent prototype definition evaluates both bounds on all three local
compiler versions. Integer and float narrowing and callback controls also pass.
The [saved evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/gcc14-parameter-bounds/summary.json)
contains original failing macOS logs, local commands, compiler versions, source,
and binary hashes. Apple Clang passes the original macOS runtime fixture.

The native fixture recognizes only the missing old-style bounds on these exact
recorded GNU builds. It still requires successful prototype effects, narrowing,
and callbacks, and reports the discrepancy in the dedicated CI step. Other
outputs, crashes, and compiler versions fail normally; a compiler that fixes the
bug passes normally. The checked graph continues to require all four bounds in
the two definition forms. This exception supplies no runtime-equivalence evidence
for the affected old-style definition. The prototype controls subsequently passed on both native macOS architectures;
the logs are retained with the [vector-initializer observations](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/vector-initializer-oracles-2026-09-08/summary.json).

## Static vector conversions across Clang releases

Apple Clang 17 (`clang-1700.0.13.5`) accepts a file-scope float-vector initializer
formed by converting an integer-vector compound literal. GCC 13.3 and upstream
Clang 18.1.3 reject the same source. Both macOS unit jobs exposed the stale
assumption that this extension must be rejected by every Clang release.

The [recorded source and compiler observations](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/vector-initializer-oracles-2026-09-08/summary.json)
preserve both macOS logs and fresh local diagnostics. Toucan currently diagnoses
static vector-expression evaluation as unsupported; that is an implementation
gap, separate from the native compiler's source constraints. The focused regression
requires that explicit diagnostic. Native conversion constraints and runtime lane
checks remain enabled. Static vector evaluation and compiler-version differences
need their own implementation and value probes before broader support is claimed.

## Apple Clang resource headers on musl targets

Apple Clang 17's resource `stddef.h` uses `#include_next <stddef.h>` for musl
cross targets, including `-ffreestanding -nostdinc`. The isolated `stdatomic.h`
oracle intentionally has only the compiler's resource include directory, so that
route requires a target sysroot it does not have. This is a header configuration
failure, not a C source rejection or a frontend conformance result.

On macOS, CI selects upstream Clang 18 with `TOUCAN_CLANG_RESOURCE_ORACLE` for
both musl targets in `unchanged_clang_stdatomic_header_and_operations`. The test
obtains that compiler's own resource directory and passes the same untouched
headers to Toucan and that compiler. The other targets continue to use the host
Clang and its headers. Set this variable to an upstream Clang path when running
this isolated test locally with Apple Clang; no headers are copied or edited.
The separate native musl jobs validate actual musl sysroots and runtime calls.
The [saved local validation](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/apple-musl-resource-oracle-2026-09-08.json)
records the failure, oracle selection, test results, and untouched header hashes.

## Apple Clang's C90 implicit-declaration diagnostic

Apple Clang 17 (`clang-1700.0.13.5`) treats implicit function declarations as
errors by default even with `-std=c90` or `-std=gnu90`. The C90 scope and generated
FFI tests exercise this valid language behavior, so their compiler commands use
`-Wno-error=implicit-function-declaration`. Invalid redeclarations still require
matching compiler rejection. C99 and later language checks are unchanged.

The [evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/c90-oracle-policy-2026-09-08/summary.json)
preserves the original Intel macOS failures and local controls that reproduce
the diagnostic severity with Clang. The corrected scope checks and C/Rust calls
pass locally, including actual Rust 1.64. Those local results do not replace
native macOS CI validation.

## Native ARM fixture link order

The native AArch64 GNU linker can discard libc under `--as-needed` when a C
object passed through Rust's `-C link-arg` appears after the system libraries.
GCC 13's stack-protector references then fail to resolve. The FloatN fixture
exposed this with an unresolved `__stack_chk_guard` and `DSO missing from command
line` error.

The shared test helper now archives each C object and supplies it through Rust's
native static-library options. This places the fixture before the system
libraries while retaining its original compiler flags and stack protection.
The FloatN, non-object query, and omitted-conditional FFI fixtures all passed
the [native ARM job](https://github.com/astral-sh/toucan/actions/runs/34276674090/job/102231280883)
with GCC 13.3.0, Clang 18.1.3, and Rust 1.98.1.

The [saved log and source identities](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/native-link-order-2bbc3ef/summary.json)
tie the run to PR 227 head `2bbc3efd`. GitHub's checkout merge commit `eb5607cc`
has the same source tree as that head. This completes native ARM validation of
the repair after the earlier local cross-link and emulated runtime checks.
