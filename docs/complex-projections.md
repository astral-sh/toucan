# Complex component operations

GNU `__real`, `__real__`, `__imag`, and `__imag__` accept arithmetic operands.
A complex lvalue projects to a corresponding-real lvalue; a complex value
projects to a real value. Real-scalar `__real__` preserves the operand, while
`__imag__` produces zero after evaluating it. Calls, increments, and volatile
reads in that operand are preserved. `~` conjugates complex values;
increment/decrement changes their real component by one.
[GCC documents the operators](https://gcc.gnu.org/onlinedocs/gcc/Complex.html).

## Types and access

The projected component of a qualified complex object has an **unqualified**
real type in both compiler profiles. For example, `&__real__ z` has type
`double *` even when `z` is `volatile double _Complex`. Its loads and stores
remain volatile. Checked `Expression::is_volatile_place()` exposes that storage
property separately from the type; the parent `ExprUse` determines whether an
access occurs. A `Place` use alone does not read the object. The unary operand
edge retains the original object, allowing analyzers to trace writes back to
const storage even though both compilers accept such component assignments.
[Clang's operand checker](https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-18.1.3/clang/lib/Sema/SemaExpr.cpp)
and the recorded LLVM loads/stores establish this distinction.

Real-scalar projection preserves qualifiers. GCC also preserves a real bitfield
lvalue, while Clang converts a bitfield or vector-element operand to a value.
Register-address restrictions remain in force. GCC preserves ordinary complex
operand qualifiers on unary `+`, `-`, `~`, and increment/decrement result types;
Clang drops them. Both profiles retain whole-object atomic read-modify-write
semantics for atomic complex increment/decrement.

Atomic projection remains explicit: Clang rejects atomic operands. GCC exposes
nonatomic components of atomic complex objects; Toucan diagnoses that extension
until its component-access contract is represented. GNU imaginary projection of
a bitfield also diagnoses because its precise-width result needs a distinct C
integer type. Neither operation is treated as an ordinary whole-object load.

## Addresses and object-size queries

Clang accepts a static initializer such as `double *p = &__imag__ global_z`;
GCC rejects it. The profile-specific check is separate from ordinary member
addresses. Automatic component addresses work in both profiles.

Object-size inference retains the component's actual storage range. A real
component of complex `double` has eight bytes. Clang's subobject query modes
instead report the remaining enclosing complex extent: sixteen bytes at the
real component, eight at the imaginary component. GCC reports eight in both
cases. The structural proof and compiler scalar answer remain separate.
Pointer arithmetic retains existing compiler-fold-stage and uncertainty rules;
no unknown extent is replaced with an invented maximum.

## Builtins and constants

The nine `__builtin_creal`, `__builtin_cimag`, and `__builtin_conj` spellings,
including `f` and `l` suffixes, use their fixed complex parameter types and
corresponding real/complex results. Argument conversions, evaluation, and builtin
identity are retained. Unary projection and conjugation preserve component bits,
including signed zero and NaN payloads.

GCC accepts these builtin calls in constant initializers. Clang's builtin
metadata omits constant-evaluation support, and Clang18 rejects their static
initializers. Toucan accepts their runtime use and diagnoses Clang frontend
constant evaluation. It does not export optimizer-folded values as frontend
constants. The ordinary GNU unary spellings still fold in both profiles.
[Clang's builtin definitions](https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-18.1.3/clang/include/clang/Basic/Builtins.def)
record the distinction.

Projection of an integer constant retains integer-constant-expression status.
Broader GNU early folding remains a follow-up: GCC accepts `(int)__real__ 3.0`
and `(int)__imag__ (1.0 + 2.0)` as integer constant expressions, while Clang's
C11 oracle with `-pedantic-errors` rejects them. Toucan currently diagnoses these forms in required
integer-constant contexts; ordinary arithmetic evaluation supports their values.
No general early AST simplification is implied.

## Validation and remaining headers

The [compressed evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/complex-projections-2026-09-08.json.gz)
records compiler identities, original sources and commands, seven-profile
constraint/retention checks, GCC/Clang O0/O2 component values and side effects,
volatile lowering, parser regeneration, sanitizer replay, and allocation/timing
comparisons against the frozen complex core.

Untouched native glibc `complex.h`, FFTW 3.3.10 (both array and complex
selections), and LAPACKE 3.12.0 pass the shipped preprocessing route with
normal/retained parity. Native Clang preprocessing also passes. GCC-preprocessed
FFTW still diagnoses `__float128` and complex `mode(TC)` declarations. The shipped
compatibility predefines select different declarations from native GCC, so this
is acceptance evidence, not equivalent header output. Native glibc and LAPACKE
preprocessing pass. No headers or feature macros were rewritten for these checks.

Two-component brace initialization, further complex formats, floating-state
pragmas, and Rust complex storage/call representations remain separate layers.

The workspace suite passes 584 tests (158 native/environment tests are ignored
in that default run). The parser/semantic native suites separately pass 496
tests; the macro tests pass two.
AddressSanitizer passes the five focused cases and all 434 checked-seed/profile
pairs (326 accepted inputs, 108 matching diagnostics). Regeneration under
`PYTHONOPTIMIZE=1` reproduces the checked-in parser exactly.

All seven real translation units pass both preprocessing routes with
normal/retained parity; all 28 declaration hashes match the core baseline.
Whole-source measurements are single observations, not throughput claims.

Ordinary allocation counts and all four binding output hashes match the frozen
core. Seven alternating binding observations give median elapsed-time ratios
from 0.974 to 1.003; these shared-host measurements establish no speed improvement.
The expression-info field layout remains 64 bytes with both the volatile flag
and the pending four-byte alignment-origin ID. Public checked expression and
operand-use sizes remain unchanged.
