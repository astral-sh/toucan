# Unspecified vector values

Clang's x86-64 profiles support `__builtin_ia32_undef128()` with no arguments and
a result type of two `double` lanes. GCC and AArch64 profiles do not provide this
spelling. The intrinsic has no instruction-set requirement and no ordinary side
effects. It remains usable in unevaluated expressions and when MMX is disabled.

Checked code retains `Builtin::X86(X86Intrinsic::Undef128)` and its result type.
`X86Intrinsic::has_unspecified_result()` distinguishes its stable, unspecified
value from an ordinary instruction result. Reusing a result must preserve the
same value: a downstream backend must not substitute LLVM's per-use `undef` or
`poison`. No particular result bits are promised by this API.

The pinned Clang 18 implementation lowers the call to an all-zero vector. That
implementation choice does not make the source expression a C constant:
`__builtin_constant_p` returns zero, and static initialization and integer constant
expression contexts reject the call. Toucan does not fold it to a zero constant.
Native tests inspect only defined lanes selected from another input; separate
compiler evidence records Clang's LLVM output.

This feature does not broaden Rust vector call representations. Pointers retain
the existing vector storage layout, while unsupported by-value bindings still
receive a diagnostic. The 256-bit and 512-bit forms remain outside the current
16-byte vector policy.

Primary evidence comes from the pinned
[Clang builtin definitions](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/include/clang/Basic/BuiltinsX86.def),
[Clang lowering](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/CodeGen/CGBuiltin.cpp),
and [LLVM value semantics](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/llvm/docs/LangRef.rst).
[Validation evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/undefined-vector-2026-09-08.json) separates
cross-target compilation, native execution, and remaining project blockers.
