# Allocation builtins

Toucan checks `__builtin_malloc`, `__builtin_calloc`, `__builtin_realloc`, and
`__builtin_free` using the selected target's C `size_t` and `void *` types.
Arguments use ordinary C assignment conversions. Invalid arity, discarded pointer
qualifiers, and incompatible argument types are diagnosed. An allocation size is
not evaluated as a request to allocate memory during analysis.

Retained calls identify the `AllocationOperation` and keep each converted argument
use. Argument evaluation occurs once, with C's unspecified relative order.
Undeclared builtin calls carry the compiler's known side-effect classification.
Explicit declarations also link their retained entity; their purity gate remains
unresolved because general `pure` and `const` declaration contracts are not yet
modeled. A visible non-return promise is captured before checking later operands.

GNU allows these builtin names as function values. `ExprKind::BuiltinFunction`
identifies the corresponding C library symbol, including in address constants.
Clang requires a direct call; parentheses around the call name are allowed.
Ordinary local variables, callbacks, typedefs, and internal functions can shadow
the builtin names. Compatible external declarations keep the builtin prototype;
GNU also permits incompatible replacement declarations with its normal warning.
Clang rejects incompatible external declarations and builtin definitions.

Explicit compatible declarations receive a `Declaration::link_name` such as
`malloc`, so generated Rust calls link to the C library symbol. GNU ignores an
asm rename on those declarations. Clang honors a rename, keeps block declarations'
renames local, and rejects a rename introduced after the function's first use.
Retained builtin calls keep any symbol override. Clang's type-only uses do not
prevent a later rename; `constant_p` operands and unselected `choose_expr` or
generic arms still count as uses for this diagnostic.

C11 `_Noreturn` applies to these declarations. GNU ignores the GNU-spelled
`noreturn` attribute on malloc, calloc, and realloc because it conflicts with
their allocation attributes; the same attribute on free remains effective.

## Limits and validation scope

This layer does not enable undeclared ordinary `malloc`, `calloc`, `realloc`, or
`free` calls in C11. C90's compiler-specific library-name lookup is separate.
It does not add allocation-size proofs, ownership analysis, allocator lowering,
or retention of the GNU malloc/deallocator attribute's contract. Those attributes
remain subject to the existing attribute support limits.

GNU identifier-list definitions of allocation builtins remain explicitly
unsupported. GNU permits `sizeof` on these function values and returns 1; Clang
requires a direct builtin call, including inside a size query. GNU's special extern declaration beneath a
same-named internal object remains outside the modeled block-linkage rules.

An asm declaration nested inside an unevaluated operand is explicitly unsupported
for Clang. Its declaration-time effect can depend on an enclosing variably modified
type that has not finished checking. Ordinary symbol overrides remain supported;
array-bound uses retain their independent compiler expression context.

The symbol registry is allocated only for an explicit Clang rename. Its live
symbol payload is capped at 1 MiB and scoped entries are discarded on scope exit.
Retained symbol strings and declaration edges are charged to checked-code budgets
before allocation.

The source signatures and GNU address identity follow
[GCC's library builtin documentation](https://gcc.gnu.org/onlinedocs/gcc/Library-Builtins.html).
Clang behavior is probed against Clang 18 and its
[pinned builtin declarations](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/include/clang/Basic/Builtins.def).
Compiler source acceptance, native C execution, and generated Rust calls are
separate checks; Toucan does not generate machine code.

The [validation record](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/allocation-builtins-2026-09-08.json)
contains compiler commands, source hashes, graph comparisons, native call checks,
and allocation measurements. The five unchanged zstd inputs are regression
controls; their acceptance is not a new result of this layer.

The [combined integration](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/allocation-root-integration-2026-09-08/summary.json)
includes C90/GNU90 and Microsoft declaration attributes. Its native signature
table runs 480 comparisons across four language modes, seven Clang targets and
native GCC. It also checks 3,828 ordinary/retained seed pairs, reruns generated
allocation calls with Rust 1.64, and preserves eight binding reference artifacts.
