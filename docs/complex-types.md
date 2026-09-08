# Complex types

Toucan checks the three C11 complex scalar types: `float _Complex`,
`double _Complex`, and `long double _Complex`. They retain their own C identity,
qualifiers, corresponding real precision, and target layout in declarations and
checked code. Arrays, records, function signatures, initializers, casts,
assignments, and atomic wrappers can contain them.

Complex is an optional C11 feature. This change does not define
`__STDC_NO_COMPLEX__` or advertise Annex G conformance. Unsupported selected
header declarations produce diagnostics; feature macros do not hide them.
[The C11 draft](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf)
specifies the types and conversions in 6.2.5 and 6.3.1.6–8.

## Expressions and constants

Complex values support scalar truth, equality, arithmetic, assignment, calls,
returns, and conditional expressions. Ordered comparisons, integer-only
operators, and conversions to or from pointers are rejected. Conversion to
`_Bool` considers both components; conversion to another real type discards the
imaginary component. Conversion from a real type adds positive imaginary zero.
Complex `float` remains complex `float` in a variadic call or an identifier-list
function's incoming argument type.

The usual arithmetic conversions select a common real precision while preserving
each operand's real or complex domain. For example, multiplying `(infinity, 1)`
by the real value `2` produces `(infinity, 2)`. It does not introduce an imaginary
zero multiplication. Retained operand uses preserve this distinction, including
compound assignments. [Clang's complex lowering](https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-18.1.3/clang/lib/CodeGen/CGExprComplex.cpp)
uses separate formulas for mixed operands.

GNU imaginary floating literals and `__builtin_complex(real, imaginary)` are
supported because system `complex.h` macros use them. The constructor requires
matching real floating types and preserves both components directly, including
signed zeros and NaN payloads. It evaluates each argument once and retains the
constructor as a distinct builtin operation.
[GCC documents these spellings](https://gcc.gnu.org/onlinedocs/gcc/Complex.html).

`evaluate_arithmetic` returns `ArithmeticConstant::Complex(ComplexValue)`. The
value exposes its C kind, target format, and two `FloatingValue` components.
These are target bit encodings, excluding padding within long-double storage.
Complex expressions do not become C11 integer constant expressions merely
because an explicit cast produces an integer.

Constant operations use `rustc_apfloat`, with no host floating-point arithmetic.
Clang-profile multiplication and division use target-rounded formulas, scaled
denominators, and exceptional-value recovery. GNU's full-complex constant
multiplication and division use correctly rounded component results where the
frontend can prove them: outward-rounded binary128 intervals must round to the
same destination value at both endpoints. Unproved results produce a specific
constant-evaluation diagnostic. This can occur with extended-format extremes or
hard rounding boundaries; ordinary runtime expressions remain type-checkable.
Static initializers that require those unavailable folds also diagnose.

This distinction matters. GCC13 folds
`(1e308 + 1e308i) / (1e308 + 1e308i)` to `(1, +0)`, while Clang18 folds it to
`(infinity, +0)`. The permitted intermediate-overflow differences do not justify
exporting an arbitrary constant under a selected compiler profile. GNU full
complex multiplication/division also canonicalizes NaN payloads through MPC;
Clang preserves operand payloads. The tests compare each profile to its compiler.
[Clang's constant evaluator](https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-18.1.3/clang/lib/AST/ExprConstant.cpp)
and [GCC's folding implementation](https://github.com/gcc-mirror/gcc/blob/releases/gcc-13.3.0/gcc/fold-const.cc)
provide the implementation references. Arithmetic on signaling NaN constants
retains the existing explicit unsupported diagnostic.

Clang complex `__builtin_constant_p` operands are supported when Toucan proves
them constant before the compiler's scalar fallback. Unproved complex query
operands diagnose: Clang18 crashes when its fallback attempts to emit a complex
operand as a scalar expression, including fresh VLA bounds. GNU query operands
remain fully unevaluated. The retained query policy records the proven frontend
fold instead of inventing runtime evaluation.

## Storage, atomics, and Rust bindings

C complex storage consists of two corresponding real components, real first,
with the same alignment as that real type. Atomic complex uses the target's
atomic alignment: for example, ordinary complex `double` has size/alignment
16/8, while atomic complex `double` has 16/16. Checked atomic loads, stores,
copies, and compound assignments preserve whole-object access. An atomic load
conversion in a read-modify-write use belongs to that single update.

Clang18's generated code for atomic complex compound assignment uses separate
atomic load/store calls, unlike GCC's compare-exchange loop. Toucan retains the
C11 read-modify-write requirement, rather than reproducing that lowering defect.

Direct Rust bindings currently diagnose complex storage and calls. In particular,
a two-field Rust struct cannot be assumed to use the complex call ABI: on
x86-64, long-double complex and a two-field long-double struct have equal layout
but different return conventions. The binding checks follow aliases, callbacks,
arrays, records, and atomic wrappers so nested complex values cannot evade this
boundary. Safe selected interfaces remain generatable. Proven pointer/storage
representations and target-specific call ABIs are separate follow-up layers.
[Clang's x86 ABI implementation](https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-18.1.3/clang/lib/CodeGen/Targets/X86.cpp)
and the [Rust complex interoperability goal](https://goals.rust-lang.org/2026/interop-complex.html)
describe why layout equivalence is insufficient.

## Remaining extensions

[GNU component operations](complex-projections.md) support projection,
conjugation, increment/decrement, and the fixed projection/conjugation builtins.
Integer complex, half/extended complex formats, and two-component brace
initializers remain unsupported. GNU atomic component projection needs a
separate model: GCC exposes nonatomic component storage, while Clang rejects it.
`STDC` floating-state pragmas retain explicit diagnostics. None is silently
ignored by this layer.

The tests cover seven compiler/target profiles, native GCC/Clang constraints,
static component bits at O0/O2, target precision, typed operand uses, atomic
updates, and binding rejection paths. Cross-target compiler checks establish
frontend and layout behavior; they do not substitute for native execution.

## Recorded validation

The [compressed evidence](../corpus/evidence/complex-types-2026-09-08.json.gz)
contains compiler identities, original probes, source and artifact hashes, test
logs, and before/after measurements against revision `2b5eb1f`. All seven actual
translation units pass both preprocessing routes with normal/retained parity;
all 28 declaration hashes match the baseline. The four binding outputs are
byte-identical. Seven alternating binding observations put median elapsed-time
ratios between 0.996 and 1.012; these shared-host measurements establish no speed
improvement. Whole-source timings are single observations, not benchmark claims.

Ordinary semantic allocation counts are unchanged. Binding selection through
records adds one lazy memo allocation (eight bytes in the small measured record
fixtures), which prevents repeated traversal of shared record graphs.
`ArithmeticConstant` grows from 48 to 64 bytes; `Type` and checked expression/use
sizes are unchanged. AddressSanitizer passes the complex/depth cases and all 427
checked-seed/profile combinations, including 319 accepted inputs and 108 matching
normal/retained diagnostics. This is bounded replay, not a sustained fuzz campaign.

The [integration report](../corpus/evidence/complex-types-integration-2026-09-08.json)
records 756 passing workspace tests, including native checks, after vector
shuffles and minimum-vector-width attributes. All 448 checked-seed/profile
comparisons preserve ordinary/retained declaration parity and graph invariants.
