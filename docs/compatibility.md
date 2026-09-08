# Compatibility

Toucan is an experimental C header frontend. The repository contains working
bindings and independent probes for substantial public APIs; it is not yet a
production-ready replacement for bindgen or a complete C compiler frontend.

## Supported scope

The current declaration pipeline handles common C11 declarations: scalar types,
pointers, arrays, typedefs, structs, unions, enums, prototypes, callbacks, variadic
declarations, external variables, integer constant expressions, and selected GNU
extensions.
Layout supports packing, explicit alignment, and bitfields. Binding generation has
additional representation constraints described below.

GNU `__int128` and `unsigned __int128` work in declarations, casts, constants,
and function bodies. Compiler probes cover scalar, packed-record, and bitfield
layouts on all five targets, including Clang's Windows extension; Microsoft C
does not accept this GNU spelling. Generated 128-bit ABI types require Rust 1.78
or newer with its bundled LLVM, following Rust's
[ABI correction](https://blog.rust-lang.org/2024/03/30/i128-layout-update/).

## String and character literals

C11 ordinary, `u8`, `u`, `U`, and `L` strings use UTF-8, UTF-16, or UTF-32
code units according to their prefix and target. Adjacent ordinary strings inherit
the other literal's prefix. Differently prefixed wide strings are rejected.
Octal and hexadecimal escapes preserve code units, including bytes that are not
valid UTF-8; universal character names must name valid C11 Unicode scalars.
Array bounds include the terminating NUL unless an explicit bound omits it.

Character constant types and values use the target's plain `char` and `wchar_t`
widths and signedness. GNU profiles follow GCC's implementation-defined values for
multicharacter constants; Apple and Windows profiles reject wide constants that
require multiple code units, following Clang. Alternative execution character
sets and C23 `u8` character constants are not implemented.

The semantic literal tests compare values with native GCC and Clang and types and
array bounds with Clang across all five targets. They also check malformed escapes,
incompatible array element types, and the public decoder's input-size limit.

## Arithmetic constants

`evaluate_integer` checks C integer constant expressions. `evaluate_arithmetic`
also accepts supported floating expressions and returns an owned integer or floating
value. An integer result from the latter query does not make the source a C integer
constant expression: `(int)(1.0 + 2.0)` is one example.

Floating literals, arithmetic, and casts round to nearest, ties to even in their
target format without host floating-point arithmetic. The public result retains
the C type, encoding, and exact bits, including negative zero and subnormals.
`long double` uses x87 extended precision on x86-64 Linux/macOS, binary128 on
AArch64 Linux, and binary64 on AArch64 macOS and x86-64 Windows. Its bits exclude object padding.
The `__builtin_inf` and `__builtin_huge_val` families preserve target infinities.
Overflow, division by zero, invalid operations, and unsupported formats produce
diagnostics.

The `__builtin_nan` and `__builtin_nans` families retain quiet/signaling NaN payloads
in each target format. Constant payloads may be ordinary or UTF-8 string literals
with unsigned decimal, octal, or hexadecimal digits, optionally wrapped in char or
void pointer casts. Payloads are truncated to the target significand; empty strings
select the compiler default. Runtime payload expressions are type-checked as
`const char *` arguments. Leading signs/whitespace, embedded NULs, and nonliteral
payload expressions are unsupported in constant evaluation.

Copies and unary signs preserve signaling bits. Casts to a different floating
format quiet signaling NaNs; binary64 `long double`/`double` casts preserve their
bits. Arithmetic on signaling NaN constants is explicitly unsupported because
compiler folding can differ by operation and optimization settings. Quiet-NaN
arithmetic, comparisons, and boolean conversions are supported. Integer casts of
either NaN kind report an out-of-range value.

Rust bindings emit `float` and `double` macro values as `f32` and `f64`
using exact bit patterns. For Rust 1.83 and later, emission uses `from_bits`;
Rust 1.64–1.82 uses an equal-width const transmute because `from_bits` was not yet
const-stable. Every integer bit pattern is valid for the corresponding IEEE float.
`long double` macro values remain explicitly unsupported in Rust bindings, even
on targets where they use binary64. A C cast to `float` or `double` selects an
emittable type and applies the target's conversion rules.

Tests compare bit patterns against Clang for all five targets, including meaningful
`long double` bits, and execute generated constants against native GCC/Clang FFI
calls with current Rust and Rust 1.64. These checks do not cover alternate rounding
modes, excess-precision options, or floating-point environment access.

## Variable-length arrays

Variable-length arrays retain a distinct runtime-sized type. Bounds, parameter
adjustment, storage and scope constraints, and jumps across variably modified
declarations are checked. `sizeof` on a VLA is valid at runtime and cannot be used
as an integer constant expression; its alignment is available without a runtime
bound. Rust bindings support parameters whose outer array layer adjusts to a
pointer, and reject remaining runtime-sized array layers. The optional
[checked graph](analysis-api.md) exposes runtime bound expressions, their evaluation
contexts, and typed bodies.

## Flexible-array initialization

Named static objects may initialize a flexible array under the target compiler's
GNU extension rules. `Declaration.flexible_array_storage` records the selected
member, element count, and allocated bits for that object. The declared record
layout, flexible member type, and `sizeof` remain unchanged. Padding, designators,
and repeated initializers follow the GCC or Clang profile and are checked against
C object-symbol sizes. Anonymous compound-literal storage with an initialized
flexible member remains unsupported.

## GNU pointer conversions

The supported targets accept GNU conversions between function pointers and
`void *` in assignments, calls, equality comparisons, and conditional expressions.
The common pointed-to type is `void`; GCC retains the void operand's qualifiers
and Clang drops them for conditional expressions combining these pointer types.
Conversions to object pointers still reject discarded qualifiers. As in GCC and
Clang, converting a qualified `void *` to a function pointer does not qualify the
function type. Other incompatible pointed-to types and multiple pointer levels
remain errors. These function-pointer conversions are an extension to C11.

Relational comparisons between qualified `void *` operands follow the GNU
extension used by SQLite. Retained comparisons preserve the combined pointer
qualifiers. Relational comparisons between distinct pointed-to types and between
function pointers remain unsupported.

## Compiler intrinsics

The `stdarg.h` built-ins check argument-list types using the target's array,
record, or pointer representation. `va_start` requires a variadic function and its
visible last named parameter; `va_arg` requires a complete result type. List
mutation requires the appropriate pointer or modifiable object. These checks do
not prove runtime list initialization, lifetime, or agreement with the caller's
actual variadic arguments.

`__builtin_expect` checks both arguments against the target's `long` type. When
both are arithmetic constants, evaluation returns the converted first argument.
`__builtin_unreachable` and `__builtin_trap` have void type and take no arguments.
Ordinary declarations can shadow intrinsic call names.

GNU [`__builtin_types_compatible_p` and `__builtin_choose_expr`](type-introspection.md) preserve type identity, selected value categories, and unevaluated type/arm operands. Their checked operations retain both written types or all three selection expressions.
GNU [`__auto_type`](auto-type.md) infers initialized object declarations with compiler-specific name visibility and atomic qualification. Clang profiles also support derived pointer, array-pointer, and function-pointer patterns and multiple declarators with one exact deduced base type. GNU profiles retain the single plain-identifier rule. Retained sites link initializers, callback declarations, and original runtime array bounds. Explicit Clang `_Atomic __auto_type` deduction remains diagnosed when the compiler does not produce a concrete type.

`__builtin_constant_p` checks one value operand and returns `int`. Its conservative
constant evaluator returns one only for supported, valid constant folds; zero
means this frontend did not prove the operand constant. This includes arithmetic
constants, literal strings, supported pointer casts, and folded conditional or
short-circuit expressions. Object-value propagation, statement-expression values,
and compound-literal values are not proofs. This policy does not predict GCC or
Clang optimization: both compilers can change a local-variable query from zero to
one at `-O2`. The retained query preserves its operand and C array/function
conversions. See [GCC's constant-query contract](https://gcc.gnu.org/onlinedocs/gcc/Other-Builtins.html).

GCC-profile queries suppress nested VLA bounds and `typeof` operands. Clang uses a
conditional scalar-evaluation fallback. For example,
`__builtin_constant_p(sizeof(int[n++]))` can increment `n`, and
`__builtin_constant_p(sizeof *(p++))` can advance a pointer to a VLA. Clang first
attempts constant evaluation of the intrinsic. If that does not finish, it checks
for ordinary operand side effects, deliberately ignoring `sizeof` and `_Alignof`
children, and emits the scalar operand only if the gate permits it. Ordinary side
effects anywhere in the operand, including dead conditional branches, suppress
this fallback. The fallback itself honors short-circuit and conditional execution.
Clang also accepts void and incomplete-record operands, which GCC rejects.

Retained `QueryEvaluation` and `UseContext::CompilerQuery` describe this boundary;
no query result predicts that a compiler reaches its fallback. Required constant
expressions have no runtime fallback. The existing expression tree retains guards,
VLA `sizeof` operands, fresh type bounds, and nested queries without duplicating
side effects or imposing an order on unsequenced operands. Individual bound and
`typeof` evaluation facts remain subject to their enclosing expression's reachability
and query policy. The gate is explicitly unresolved for ordinary calls (whose
`pure`/`const` attributes are not retained), compound literals, and statement
expressions. Consumers must preserve both possibilities in those cases; this API
does not claim optimizer-equivalent effect analysis. See Clang 18's
[query code generation](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/CodeGen/CGBuiltin.cpp)
and [ordinary-side-effect test](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/AST/Expr.cpp).

`__builtin_object_size` and `__builtin_dynamic_object_size` check their
`const void *` and `int` parameters and return the target's `size_t`. The mode must
be a supported constant from zero to three after conversion to `int`; GNU profiles
also accept foldable floating and comma expressions. Retained calls distinguish
the two operations and preserve their argument conversions and query evaluation
policy. Clang mode three suppresses the scalar fallback; other modes use the same
side-effect gate and constant-fold boundary described above. Native probes cover
pointer increments, fresh VLA type operands, and `alloc_size` allocator calls with
side-effecting size arguments.

Object-size inference follows fixed-size named objects, record and union members,
array-element addresses, literal strings, pointer casts and supported constant
offsets. Wide strings use target code-unit widths and include their terminator.
Retained `ObjectSizeProof` separates complete-object and closest-subobject byte
ranges from the compiler query's scalar result. A known default sentinel has
`is_default: true`; it is never reported as an object extent.

Folds distinguish frontend constant evaluation from later code generation.
`evaluate_integer` and `evaluate_arithmetic` can return supported later folds;
that does not make them valid C integer constant expressions. For example, Clang
accepts a direct array query in a static initializer, while GCC does not. Clang's
string-literal modes zero and two require later folding. A frontend fold suppresses
fresh VLA type effects; a later known value retains its conditional evaluation
policy. This distinction is covered by native constant-context and side-effect
probes. See [Clang's constant evaluator](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/AST/ExprConstant.cpp).

Compiler-dependent answers remain explicit. GCC's subobject answers for pointer
arithmetic can differ between `-O0` and `-O2`; these retain geometric proofs with
an unresolved scalar result. Array decay also has a recorded profile difference:
for `char a[3][4]`, mode one on `a[1]` yields eight in GCC and four in Clang.
Mutable pointer aliases, runtime extents, allocator provenance, and unsupported
out-of-range designator recovery remain unresolved. Dynamic queries retain their
runtime evaluation policy and existing VLA-bound links. No maximum size is
invented for an unidentified object. See [GCC's object-size contract](https://gcc.gnu.org/onlinedocs/gcc/Object-Size-Checking.html).

Direct calls to `__builtin_memset`, `__builtin_memcpy`, `__builtin_memmove`, and
`__builtin_memcmp` use their C library prototypes, including the target's `size_t`
type and pointer qualifiers. Retained calls identify the operation and preserve
each argument's conversions. Memory-content constant evaluation and using these
builtin names as function values remain unsupported.

Fortified memory/string intrinsics check the `memcpy`, `memmove`, `mempcpy`,
`memset`, `strcpy`, `stpcpy`, `strcat`, `strncpy`, `stpncpy`, and `strncat` families
with explicit object-size arguments. Fortified `printf`, `fprintf`, `sprintf`,
and `snprintf` calls and their `va_list` variants retain the target's fixed
parameters and ordinary variadic promotions. GCC's stream parameter is `void *`;
Clang uses the file-scope `FILE` typedef. Retained calls identify each checked
operation and preserve its evaluated arguments. These facts do not prove buffer
capacity, valid format strings, or runtime fortify checks. Memory effects are not
constant-evaluated. Native C/Rust tests cover variadic calls and callbacks at
`-O0` and `-O2` on the tested host.

`__builtin_bswap16`, `__builtin_bswap32`, and `__builtin_bswap64` check and convert
their arguments to the target's exact-width unsigned types. Constant evaluation
swaps bytes after that conversion. The 64-bit result is `unsigned long` on Linux
and `unsigned long long` on Darwin and Windows; both are 64 bits on these targets.

## GNU fallthrough statements

`__attribute__((fallthrough));` and its underscored spelling annotate a null
statement in a switch. Retained statements identify the enclosing switch; the
annotation does not transfer control. Empty statements, lexical blocks, typedefs,
tag declarations, function prototypes, and static assertions can precede the next
case or default. Annotations in `if` branches retain their separate paths.

Placement checks reject intervening object declarations or executable statements,
loop and statement-expression boundaries, and a missing following switch label.
This is a supported common subset: GCC and Clang differ on some placements and
when they diagnose them. Duplicate annotations, multiple attributes, and other GNU
attributes on null statements remain unsupported. Native tests compile valid cases
with both compilers and check rejected placements with Clang.

## GNU statement expressions

Statement expressions (`({ ...; expression; })`) check their local declarations,
expressions, and control flow within the enclosing function. Jumps into these
blocks and jumps into VLA scopes are rejected. Their result categories and loop
condition behavior follow the target's GCC or Clang profile. Constant evaluation
of statement-expression bodies and GCC's precise-width bitfield result types
remain unsupported and produce diagnostics.

## Functions that may return more than once

GNU `returns_twice` and `__returns_twice__` accept no arguments and require a
function declaration or definition. Toucan preserves the merged property in
`Declaration::returns_twice`, retained entities, and declaration sites; written
sites also expose their attribute source span. The attribute does not become a
C function-pointer type qualifier. GCC permits adding it after a definition;
Clang diagnoses and ignores that late annotation, which Toucan rejects on Clang
profiles. Combinations with GNU `noreturn` or C11 `_Noreturn`, and multiple C names
sharing a returns-twice assembler symbol, have explicit unsupported diagnostics.

These are declaration facts. In Clang, an attribute added after an earlier call
or on a block declaration need not mark other call instructions retroactively.
The graph preserves declaration order and scope; the final entity flag is not a
compiler-lowered call-effect result. Indirect-call target analysis is not provided.
GCC describes the required caller handling in its
[function attribute documentation](https://gcc.gnu.org/onlinedocs/gcc/Common-Function-Attributes.html).

A selected returns-twice function cannot be emitted as a direct Rust binding.
Rust removed the experimental `ffi_returns_twice` feature in 1.78, so neither
current stable Rust nor the supported 1.64 baseline has an applicable supported
caller annotation; see Rust's
[removed feature record](https://github.com/rust-lang/rust/blob/main/compiler/rustc_feature/src/removed.rs).
Select a C wrapper that keeps the repeated return inside C and returns once to
Rust. An unsafe Rust declaration or `C-unwind` does not supply that missing
contract. Native tests compile a wrapper containing `setjmp` and `longjmp` with
GCC and Clang at `-O0` and `-O2`, then call the generated binding from optimized
Rust. The fixture never jumps across a Rust frame or calls back into Rust.

## Debug attributes

`nodebug` and `__nodebug__` are accepted on declarations. Clang uses this
attribute to suppress debug information for functions, variables, typedefs, and
function-pointer declarations. Toucan checks its no-argument rule for those
subjects, then discards it: the analysis graph and generated Rust do not preserve
C debug-information policy. Types, layouts, linkage, and call signatures are
unchanged. [Clang's subject definition](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/include/clang/Basic/Attr.td#L1763).

Clang ignores inapplicable placements, including ordinary fields, parameters,
and tags, with a warning. GCC 13 ignores the unknown attribute, including any
arguments. Toucan accepts those declaration cases without reproducing the
warnings, following its existing attribute-warning policy. Standalone attribute
statements and label attributes remain unsupported here. Clang profiles advertise
`__has_attribute(nodebug)` for these source constraints; this does not imply a debug backend.
The [nodebug evidence](../corpus/evidence/nodebug-2026-09-08.json) records the
compiler checks and remaining untouched-header blocker.

## Diagnostic attributes

GNU `warning` and `error` attributes, including their underscored spellings,
are checked on function declarations. Retained analysis exposes each written
annotation's declaration site, severity, decoded message, and original source
range. Redeclarations preserve separate annotations. Conflicting severities follow
the target's GCC profile on Linux and Clang profile on Darwin and Windows.

These annotations do not change function types or generated bindings. As with
GCC and Clang's syntax-only checking, Toucan does not issue their call diagnostics:
whether a call survives optimization is outside this frontend's checking phase.
For example, an `error` call guarded by a local constant can fail compilation at
`-O0` and disappear at `-O2`. The retained annotations are written facts, not an
effective compiler message or proof that a call will be diagnosed. See the
[GCC function attribute reference](https://gcc.gnu.org/onlinedocs/gcc/Common-Function-Attributes.html).

Diagnostic messages currently support ordinary strings, adjacent concatenation,
simple escapes, and universal character names. Prefixed strings and numeric
escapes have compiler-specific interpretations and produce unsupported-feature
diagnostics. Nonfunction attachments also produce diagnostics. Clang's
`diagnose_if` and `enable_if` call constraints remain unsupported and are rejected.

## Calling conventions

Clang profiles recognize the `__cdecl`, `__stdcall`, `__fastcall`, and
`__thiscall` keywords in function declarations, callbacks, casts, and typedefs.
The MSVC profile also accepts `_cdecl`, `_stdcall`, `_fastcall`, `_thiscall`, and
`_vectorcall`. GNU profiles leave the double-underscore keyword names available
as ordinary identifiers. `__stdcall`, `__fastcall`, `__thiscall`, and `__pascal`
are ignored on the supported 64-bit targets, as in Clang; `__cdecl` retains an
explicit request for the target's default convention.

Declaration placement selects the function type. For example, if `Microsoft` is
an `ms_abi` function typedef on x86-64 Linux, `Microsoft *__cdecl f(int)` changes
the returned callback to the C convention. A convention written on a pointer can
replace the convention of its function type; a conflicting convention directly
on the function typedef remains an error. Native tests exercise generated
bindings returning both C and Microsoft callbacks, and C calls into Rust
callbacks, with optimized and unoptimized C and Rust.

`__vectorcall` and `__regcall` use unsupported x86-64 ABIs and produce explicit
diagnostics. AArch64 ignores these keywords as Clang does. Accepting these
keywords does not establish Windows SDK header or native Windows runtime
coverage.

Function types retain GNU `ms_abi` and `sysv_abi` attributes on x86-64. Compatibility
checks distinguish the two conventions and follow the target compiler profile for
attribute placement and inherited declarations. Rust declarations, function typedefs,
and callback pointers use `extern "C"`, `extern "win64"`, or `extern "sysv64"` as
appropriate. An explicit convention matching the platform default uses Rust's C ABI.

The x86-32 `cdecl`, `stdcall`, `fastcall`, and `thiscall` attributes have the platform
ABI on the supported 64-bit targets. Other conventions remain unsupported. The
`ms_abi` and `sysv_abi` attributes are currently rejected on AArch64; in particular,
Clang's AArch64 `ms_abi` changes the convention and cannot safely be discarded.

Clang IR probes cover all five targets. Native x86-64 GCC/Clang tests call C from
Rust and Rust callbacks from C with mixed register/stack arguments and aggregate
returns. Variadic extern declarations are checked by rustc; these tests do not
establish nondefault-ABI variadic argument traversal. Ordinary `va_start` in a
function with a nondefault ABI is rejected; the corresponding target-specific
argument-list built-ins remain unsupported.

## Targets

The canonical target triples accepted by `--target` are listed below. The default
compiler ABI is used; flags such as `-fshort-enums` are not implied.

| Target | Layout model | Native validation |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Implemented; GCC and Clang probes | [C/FFI and differential checks passed](../corpus/evidence/native-06cefbe/summary.json) |
| `aarch64-unknown-linux-gnu` | Implemented; Clang cross-target probes | [C/FFI and differential checks passed](../corpus/evidence/native-06cefbe/summary.json) |
| `x86_64-apple-darwin` | Implemented; Clang cross-target probes | [C/FFI and differential checks passed](../corpus/evidence/native-06cefbe/summary.json) |
| `aarch64-apple-darwin` | Implemented; Clang cross-target probes | [C/FFI and differential checks passed](../corpus/evidence/native-06cefbe/summary.json) |
| `x86_64-pc-windows-msvc` | Implemented; Clang cross-target probes | Not run; MSVC header syntax is incomplete |
| `x86_64-unknown-linux-musl` | Implemented; GCC and Clang probes | [Native ABI and zstd Builder checks passed](../corpus/evidence/musl-native-019012e/summary.json) |
| `aarch64-unknown-linux-musl` | Implemented; GCC and Clang probes | [Native ABI and zstd Builder checks passed](../corpus/evidence/musl-native-019012e/summary.json) |

The recorded [native run](https://github.com/astral-sh/toucan/actions/runs/34216957161)
passed on 2026-09-08, using GCC 13.3.0 on Linux and Apple Clang 17.0.0 on macOS.
The evidence identifies the tested checkout, PR head, and executable for each
configuration. See CI results for changes made after that run.

## Fixed-size GNU vectors

`vector_size` defines a distinct vector type with integer or floating lanes. The
supported byte counts are 1, 2, 4, 8, and 16, and must be multiples of the scalar
size. Layout, positional initializers, lane indexing, arithmetic, bitwise
operations, shifts, comparisons, scalar broadcasts that preserve the operand's
value, and equal-size vector/integer reinterpret casts are checked. Vector lanes
do not undergo integer promotion. Retained expressions expose broadcast conversions
and identify vector-element lvalues.

Linux profiles follow GCC's signed-character and `long` comparison-mask types;
Darwin and Windows follow Clang's `char` and `long long` masks. GNU Linux also
accepts addresses of vector lanes and vector increment/decrement. Clang profiles
reject those operations. Except for the native NEON/GNU identity conversion described below, implicit
conversions between different vector types, enum lanes, non-power-of-two sizes, and extended-vector swizzles remain unsupported.
The instruction intrinsics supported below have their own argument conversions. Static vectors accept brace initializers,
compound literals, and the profile-specific operations described in
[fixed-vector constant evaluation](vector-constants.md). The separate value query
can fold numeric lane conversions without declaring them valid static initializers.

Vectors above 16 bytes need compiler and CPU-feature configuration: GCC's default
x86-64 pointer alignment and field alignment differ, and `-mavx` or `-mavx512f`
changes the former. Toucan diagnoses these widths instead of choosing an ABI.
The [probe record](../corpus/evidence/vector-types-2026-09-08.json) preserves the
wider-vector measurements for that work.

Rust bindings use aligned byte storage for vector objects and pointers. They reject
vectors or containing records passed by value, including callbacks: stable Rust
cannot express their vector calling convention. Packed records containing vectors
and aliases with different pointer/field alignment also receive diagnostics.
For example, Windows `vector_size(16), aligned(1)` keeps 16-byte field alignment
with 1-byte pointer alignment. Native GCC/Clang tests exercise memory access and
callbacks in both directions with current Rust and Rust 1.64; this proves pointer
FFI behavior, not execution of Toucan's retained expression graph.

## x86 MMX, SSE, and SSE2 intrinsics

The 287 builtin names used by GCC 13.3's `mmintrin.h`, `xmmintrin.h`, and
`emmintrin.h` have explicit signatures and retained `Builtin::X86` identities.
They cover arithmetic, packing, comparisons, shifts, vector construction and
extraction, loads/stores, fences, and floating-point control-register access.
Non-x86 targets reject these names. Ordinary declarations can shadow them; the
builtin itself is not an exported binding.

`X86Intrinsic::from_name` identifies a spelling and `signature` exposes the target
profile's C types. The pinned Clang profile supports 213 of these names; GCC-only
spellings receive an explicit diagnostic there. GCC and Clang use different vector
types for several bitwise operations, shift counts, and memory arguments. Clang
permits equal-sized vector reinterpret conversions in these arguments, retained as
`IntrinsicArgument`; GCC requires compatible vector types. `required_features`
records MMX, SSE, and SSE2 requirements. The fixed x86-64 profiles enable these
instruction sets. Per-function target attributes and disabling CPU features remain
unsupported configuration.

Immediate constraints include argument index, inclusive range, divisibility, and
compiler stage. Clang checks integer constant expressions during analysis. GCC
may only resolve a valid operand after inlining, so analysis preserves an
`AfterInlining` obligation; consumers must discharge it before emitting
instructions. The bounds apply after conversion to the formal parameter type.
For example:

- `vec_ext_v2si` requires index 0..=1.
- GNU `pslldqi128`/`psrldqi128` take a bit count in 0..=2040 divisible by 8.
- GNU shuffle immediates retain a full converted `int` and use the instruction's
  low selector bits. Clang `shufps`/`shufpd` require 0..=255/0..=3. Clang `pshufw`
  converts its constant to `char` first; all resulting byte patterns are valid.
- GNU `prefetch` requires all three hint operands to become constants. Its
  `conditional_immediate_constraints` require locality 2 or 3 when the final
  argument is 1, selecting instruction prefetch. With the default CPU profile,
  that mode is a no-op; enabling the instruction requires separate target-feature
  and addressing support. Data-prefetch locality outside 0..=3 uses zero, as GCC
  specifies through its diagnostic. Address evaluation must still be preserved.

Runtime packed-lane shift counts remain valid, including the builtin names ending
in `i`. Instruction identities retain memory/control effects; Clang's ordinary
side-effect classification is exposed separately from architectural effects.

GNU `__builtin_shuffle` has a separate `Builtin::VectorShuffle` identity. It takes
one or two vectors of the same type and an integer mask with the same lane count
and element size. Mask values select modulo the concatenated input length, including
negative indices. The result preserves the first input's typedef alignment; each
argument is evaluated once with ordinary unspecified argument order. Clang does
not provide this spelling.

These operations are type checked and retained, without SIMD constant folding or
machine-code generation. Rust bindings continue to reject vectors passed by value.
The [MMX probe record](../corpus/evidence/mmx-intrinsics-2026-09-08.json) and
[SSE probe record](../corpus/evidence/sse-intrinsics-2026-09-08.json) record exact
compiler signatures, native reference programs, untouched headers, and real zstd
translation units. Reference programs execute compiler-generated code, not Toucan's
retained graph.

## Legacy atomic intrinsics

The `__sync` family supports fetch-and-operation, operation-and-fetch,
compare-and-swap, lock exchange/release, and the zero-argument synchronization
barrier. Integer, enum, and pointer objects of 1, 2, 4, 8, or 16 bytes are checked.
Linux follows GCC's overload rules; Darwin and Windows follow Clang's. GCC
excludes `_Bool` from arithmetic operations, while both profiles allow boolean
lock and compare-and-swap operations.

GCC permits const-pointer erasure and pointer/integer conversions in these
intrinsics. Retained `IntrinsicArgument` conversions preserve their destination
types without classifying them as ordinary assignments. Clang instead checks its
value arguments with assignment constraints and rejects const object pointers.
GCC removes typedef alignment from results; Clang preserves it.

Optional trailing operands are type-checked but not evaluated. GCC performs their
ordinary value conversion, including its register-array restriction; Clang retains
them without that conversion. Retained discarded operands do not gain default
argument promotions. Retained calls identify the exact `SyncOperation`
and mark those extra argument uses as unevaluated. Atomic calls remain effectful
and cannot be evaluated as scalar constants. Pointer arithmetic uses raw byte
increments, rather than scaling by the pointed-to type.

Size-suffixed runtime aliases remain unsupported. Frontend acceptance does not
assert that an operation is lock-free or that a platform supplies its fallback
runtime symbol. The [validation record](../corpus/evidence/sync-builtins-2026-09-08.json)
includes compiler signature probes and native GCC/Clang execution that checks the
operation meanings and ignored side effects; Toucan does not generate machine
code for these operations.

## Current gaps

- Bodies and initializers are type-checked, including the supported GNU statement
  expressions and inline assembly operands. Unsupported constraints produce
  diagnostics. The optional checked graph exposes typed bodies; consumers still
  need to implement execution or lowering for its supported compiler intrinsics.
- Declaration constraints still need broader conformance testing. Prototype-local
  tags retain distinct identities and are checked against GCC and Clang.
- [C11 identifier-list definitions](old-style-definitions.md) preserve incoming
  argument types, local parameter types and retained entry conversions.
- C++, unsupported calling conventions, and unknown ABI attributes are rejected.
- The three [C11 complex scalar types](complex-types.md) have semantic, layout,
  constant, and checked-code support. Complex Rust storage and call ABIs remain
  explicit binding diagnostics.
- Thread-local and atomic objects are checked by the semantic library. Direct TLS
  bindings require C accessors. Atomic objects use the storage and scalar call
  representations described below; aggregate calls still require C pointer accessors.
- `_Float16` and `__bf16` have distinct types, layouts and supported conversions;
  see [half types](half-types.md) for constant-precision and Rust ABI limits.
  [Binary128](binary128.md) has target-aware spelling, literal, machine-mode,
  semantic, layout, constant, and checked-code support. Its Rust storage/call ABI
  remains unsupported. Other extended floating-point types retain their identity but do not have
  supported layout or binding representations. `long double` has a target layout, but its Rust
  binding representation is not implemented.
- Large enum constants follow the target's GCC or Clang profile, including their
  types during and after the definition. Apple enum ranges requiring more than
  64 bits and values that cannot be represented without truncation are rejected.
- Rust bindings reject records containing bitfields passed by value, field-level
  alignment, and combined packing and explicit record alignment. Union bitfields
  have the pointer-based representation described below; volatile bitfields and
  bitfields with Rust enum representations remain unsupported.
- Function-like macros and object macros that are not supported integer,
  `float`/`double`, or string constants are reported as omitted. SQLite's `SQLITE_STATIC` and
  `SQLITE_TRANSIENT` destructor macros are examples. No invalid function pointer is
  synthesized to represent a sentinel.
- Macro expansion diagnostics identify an invocation location; they do not yet
  expose a full nested expansion backtrace.
- The bundled resource headers are fallbacks for common header declarations, not a
  complete C standard library or compiler SDK. Supply actual target resource headers
  and sysroots when required.
- The public API and JSON schema are experimental. No release stability commitment
  or broad compiler conformance claim has been made.

Unsupported selected ABI representations fail binding generation. Reports name
selected macros that cannot be emitted, and `--deny-skipped-macros` can make these
omissions an error. Internal-linkage declarations and function definitions are
reported as skipped. They are not exposed as callable externs. A function definition can still have an
exported symbol; generating bindings for definitions is outside the current scope.

Static address initializers include array-valued subobjects, such as
`int *p = &grid[1][2][3]`, array members, and indirect function designators.
The checker preserves static storage provenance through array/function decay;
reading an ordinary scalar or pointer object still does not form an address
constant. [Regression evidence](../corpus/evidence/static-subobject-addresses-2026-09-08.json)
includes strict C11 compiler comparisons and native pointer/callback checks.

## Rust consumer compatibility

ABI equivalence alone does not guarantee that existing Rust callers compile.
The optional Rust enum, `size_t`, and integer macro policies are tested with
unmodified `zstd` and `zstd-safe` wrappers using freshly generated bindings for
`zstd-sys`. The [consumer fixture](../tools/zstd_consumer/README.md) pins the
versions used by the inspected Ruff and uv checkouts and exercises bulk and
streaming compression. Its evidence is separate from the default-output bindgen
comparison below.

The [refreshed ty and uv integration](../corpus/evidence/astral-consumers-dbda83c/summary.json)
uses bindings generated at `dbda83c`. Both pinned projects build with their locked
dependencies, changing only the zstd-sys binding files and its Cargo path override.
The selected `ty_vendored` and `uv-extract` library suites pass all 21 tests with
both original and generated bindings, with identical named results and no ignored
tests. ty produces matching valid/invalid diagnostics after loading its compressed
typeshed; uv installs and imports a zstd-encoded wheel and rejects a truncated
frame. Dependency records identify the bindings consumed by binary and test builds.
These are native Linux scenarios and selected library suites, not the full project
test suites or an end-to-end ty/uv performance comparison.

## Recorded evidence

The [upstream corpus](../corpus/README.md) processes untouched public headers from
pinned zlib, SQLite, zstd, and libgit2 releases. Every recorded target emitted 1,648
declarations, skipped no selected declarations, and passed 5,444 C/Rust comparisons.
These cover constants, enum representations, selected records, and actual FFI calls
for compression, SQLite queries, and libgit2 operations. Reports record 111 omitted
macros on Linux and 110 on macOS, with their reasons.

Bindgen comparison matched 1,284 function signatures and three global types on each
target. Independent C probes checked every complete generated record and ordinary
field: 111 records and 636 offsets on x86_64 Linux and macOS, 111 and 638 on AArch64
Linux, and 109 and 628 on AArch64 macOS. The [corpus results](../corpus/README.md#recorded-native-results)
separate those C checks from shared-field and typedef comparisons against bindgen.

The comparison gate passed with no unexplained differences. Exact API equivalence
remains false; accepted macro types, unsigned sentinels, a nested SQLite callback,
additional constants, helper names, and private storage representations remain
visible in the evidence. C oracles independently check the accepted type, value,
and callback differences.

A [separate test run](https://github.com/astral-sh/toucan/actions/runs/34177031459)
passed optimized C/Rust `va_list` calls on all four native targets. These tests
exercise argument passing in addition to size and alignment. Successful probes
establish the tested representations and calls; they do not validate every C
declaration constraint or every possible use of these APIs.

The [expanded dependency audit](../corpus/evidence/dependency-audit-2026-09-08-expanded.json)
found no known RustSec advisories or informational warnings across all five
lockfiles: the workspace, fuzzing, comparison tool, zstd consumer, and SQLite
consumer. It records the database revision and lockfile checksums; yanked package
status was not checked.

The [body-checking fuzz campaigns](../fuzz/evidence/readiness-2026-09-08.json)
processed 1,281,684 inputs across preprocessing, semantic analysis, and binding
generation after fixing exponential record-member traversal and recursive parser
stack exhaustion. Each campaign ran for 301 seconds with AddressSanitizer and
explicit input, time, and memory limits. The report records the tested sources;
it does not cover subsequent changes or establish complete safety.

## Requirements for a production release

Complete the declaration and expression conformance work; extend independent review
and adversarial testing of the [parser resource limits](parser-limits.md); extend the target/header
matrix; run sustained fuzzing and independent safety review; validate distribution
and allocator configurations; and define a stable API and diagnostics contract.
The [development plan](development.md) records the broader acceptance criteria.

## Union bitfield bindings

Unions containing bitfields retain a Rust `repr(C)` union with overlapping storage
and their ordinary public fields. Generated size and alignment assertions use
the target C layout, including packed unions and unnamed bitfields. Natural
alignment comes from a zero-sized alignment member, so an otherwise supported
union can also appear inside a packed record. Packed records containing a record
that needs Rust `repr(align)` are rejected, including containment through arrays
or intermediate records; pointer fields are permitted. C accepts such layouts,
but Rust forbids combining these representations transitively.

Bitfield getters and setters on unions are `unsafe`: another union member may
leave the bytes they access uninitialized. Each accessor requires all bytes
overlapping that bitfield to be initialized. Setters preserve the other bits in
those bytes, so they have the same initialization requirement as getters. The
implementation uses raw byte accesses without creating references to inactive
storage. Const bitfields have no setter in structs or unions. Volatile bitfields
are rejected in both because the required access width and ordering are not implemented.

Unions containing bitfields, and records containing those unions, cannot yet be
passed or returned by value, including callback arguments. Equal size and alignment
do not prove equal calling conventions. Tests compare layouts with Clang on all
five targets and initialized storage bytes with native GCC and Clang, exercising
callbacks in both directions through union pointers. Generated accessors and
layout assertions are also tested with Rust 1.64.

The [bitfield safety evidence](../corpus/evidence/bitfield-safety-2026-09-08.json)
records qualifier regressions and bounded Miri checks for partially initialized
union storage, packed access through unaligned copies, and uninitialized struct
padding. An intentionally invalid union read is rejected by Miri. These interpreter
checks cover the recorded generated Rust examples; they do not execute C FFI or
cover every generated binding.

## Aligned typedefs

GNU `aligned(N)` typedefs may increase or decrease alignment; `aligned` without
an argument uses the selected profile's default maximum. The frontend preserves
this independently of C type compatibility and canonical record-tag layout.
Arrays, local/VLA aliases, pointer aliases, fields and packing retain the target's
rules. `_Alignof` reports the declared alignment; `layout().alignment_bits` is the
minimum pointer alignment and can be smaller for an over-aligned scalar alias.
For example, `typedef int A __attribute__((aligned(16)))` has size 4 and C alignment 16;
an array of A is rejected because its element size cannot satisfy that alignment.

Rust type aliases cannot encode independent alignment. Selected aliases whose
annotations change their layout or ABI produce an explicit binding diagnostic.
Redundant annotations, including Linux's aligned(16) 128-bit integer aliases, can use
ordinary Rust aliases. Unselected aliases remain available to semantic checking.

Clang integer arithmetic and conditional expressions retain the shared typedef
ancestry that determines observable `typeof` alignment. GNU arithmetic and general
floating/complex common-type alignment remain explicit unsupported boundaries.
Combining pointer types with changed-alignment pointees also produces an explicit
diagnostic. Pointer assignment, initialization,
returns and prototype arguments preserve the destination's declared alignment and
permit compatible aligned aliases. They still reject discarded qualifiers and
incompatible nested pointer types. Integer promotions and ordinary object, pointer,
array and member uses remain supported.

[Validation evidence](../corpus/evidence/aligned-typedefs-2026-09-08.json) includes
GCC/Clang layout probes, native C/Rust int128 calls, real translation-unit progress,
and default-path allocation measurements. The real source files still encounter
separate unsupported attributes or compiler intrinsics after these typedefs.

### GNU variadic argument packs

The GNU profiles support `__builtin_va_arg_pack()` and
`__builtin_va_arg_pack_len()`, including fortify wrappers. Both have C type `int`
for type checking and unevaluated expressions. A retained `VaArgPack` operation
represents the enclosing inline function's anonymous arguments; `VaArgPackLength`
represents their count after inlining. Neither supplies a standalone runtime
integer value, and the frontend does not invent a count without a concrete caller.

A direct pack, an identity `int` cast, unary plus, or a selected generic association
in the final variadic argument position has `UseContext::VariadicPack`. Fixed and
explicit variadic arguments retain their ordinary conversions. Known pack uses in
fixed or nonfinal argument positions produce diagnostics. No-prototype callees and
empty packs are supported.

[`Builtin::requires_inline_expansion`](../crates/toucan_semantic/src/checked/expression.rs)
identifies the lowering requirement. As with GCC's syntax-only checking, ordinary
C type checking can succeed before inlining makes an intrinsic use valid or
invalid. A code generator must expand packs with the actual caller, resolve
wrappers such as comma/conditional expressions, and reject evaluated uses that
remain afterward. This library does not perform that optimization phase. Clang
profiles reject these GNU-only intrinsics. Native tests compile wrappers at `-O0`
and `-O2` and exercise empty packs, mixed promoted arguments, argument counts, and
fortified formatting through Rust calls.

See [GCC's argument-pack contract](https://gcc.gnu.org/onlinedocs/gcc/Constructing-Calls.html).

Argument-pack classification reuses the association chosen during `_Generic`
type checking. It does not recheck unselected expression trees. The semantic
selection cache permits at most 65,536 distinct generic selections per input.
The [review regression and native results](../corpus/evidence/variadic-pack-2026-09-08.json)
record the compiler probes and the corrected repeated-work case.

### GNU atomic operations

The semantic library checks all scalar and generic `__atomic` load, store,
exchange, compare-exchange, arithmetic/bitwise read-modify-write, flag, fence,
and lock-free query operations. `checked::AtomicOperation` preserves the
operation and exposes the positions of memory-order and weak-CAS operands;
retained uses carry the compiler-profile argument conversions. Pointer arithmetic
uses byte offsets. Generic operations retain their source object-pointer types
and may require a target's atomic runtime when lowered.

GCC permits same-sized generic buffers with different types and intrinsic
pointer/integer value conversions. Clang uses its ordinary pointer constraints
and additionally accepts Boolean RMW and float/double add/sub operations.
GCC qualifier-discard extensions are retained explicitly, including mutation
through a const-qualified pointer to mutable storage. Clang pointer-qualifier
constraints receive diagnostics.
Constant invalid memory orders and target-specific order modifiers such as x86
HLE receive diagnostics. Runtime orders remain explicit inputs. These operations
do not imply support for the separately diagnosed C11 `_Atomic` type ABI.

`__atomic_always_lock_free` suppresses operand evaluation, while
`__atomic_is_lock_free` evaluates its arguments. Folding only proves naturally
aligned, null-address queries of widths 1, 2, 4 and 8 and impossible sizes; other
address alignment and 16-byte CPU-feature cases stay unproven. Requiring an
unproven query as a constant produces a diagnostic rather than a guessed result.
The six `__ATOMIC_*` memory-order constants are predefined, without advertising
optional lock-free target features. Completed atomic query checks have a
65,536-entry limit; inputs without these queries allocate no query table.

[Atomic compiler and runtime evidence](../corpus/evidence/atomic-builtins-2026-09-08.json)
records the accepted/rejected compiler matrix, native operation checks and
remaining source-analysis limitations. These tests establish operand semantics;
they do not prove a code generator's concurrent-memory implementation.

### Integer overflow intrinsics

The frontend checks generic and typed `__builtin_add_overflow`,
`__builtin_sub_overflow`, and `__builtin_mul_overflow` families. The public
`checked::OverflowIntrinsic` distinguishes their mathematical operation and
source prototype. Generic arguments retain their original integer types;
typed aliases retain conversions to target `int`, `long`, or `long long`.
Result-pointer writes remain effectful. GCC pointer/qualifier extensions for
typed aliases are represented as intrinsic argument conversions. Clang generic
Boolean results use one-bit arithmetic precision despite eight-bit C storage. The
GNU conversion policy matches GCC 13 and later GCC with
`-Wno-error=incompatible-pointer-types -Wno-error=int-conversion` for these
intrinsics. [GCC 14 changed their default diagnostic severity](https://gcc.gnu.org/gcc-14/porting_to.html);
the native oracle sets these flags explicitly while leaving unknown builtins and
invalid generic result types as errors.

GNU `_p` predicates retain a discarded-value use for their third argument.
Bitfield width and signedness determine representability; plain values are
ignored while volatile reads, increments, VLA bounds, and selected branches
retain their effects. Constant folding uses exact signed arithmetic through
128-bit operands without allocating big integers. It requires constant first
operands and provable absence of third-operand effects; optimizer-dependent
folds and unresolved effects remain unproven. Predicate results preserve C
`_Bool` width and rank. Clang profiles diagnose these GNU-only predicate forms.
Completed predicate checks are capped at 65,536 entries; inputs without predicates
allocate no predicate table.

[Overflow evidence](../corpus/evidence/overflow-builtins-2026-09-08.json) records
compiler constraints, native result stores and predicate effects, constant
boundary checks, and complete SQLite/libgit2 source analysis. This is source
semantics and retained-operation evidence; no machine-code backend is implied.

### Thread-local objects

C11 `_Thread_local` and GNU `__thread` work at file scope and in block declarations
with `static` or `extern`. Every declaration of the same object must retain thread
storage; independent block-static shadows are permitted. TLS objects require
constant initializers, and a TLS object's address cannot initialize a static or
thread-local pointer. String literals and addresses of ordinary static objects
remain valid. Actual variable-length array objects cannot have thread storage;
a block-static TLS pointer to a VLA can.

GNU profiles require `static` or `extern` before `__thread`. Clang profiles accept
its reversed-order extension. Explicit `tls_model` attributes remain unsupported;
Toucan does not choose a target's TLS access or dynamic-loader model. The ordinary
and retained APIs expose thread storage independently of linkage. Binding generation
rejects selected direct TLS variables; stable Rust consumers can call generated
bindings for C accessor functions instead.

The [TLS validation](../corpus/evidence/thread-local-2026-09-08.json) includes
GCC and five-target Clang declaration probes, constant-address rejection, retained
source ownership, and concurrent Rust-to-C accessor calls with C-to-Rust callbacks.
The native harness links GCC's `libemutls_w.a` on macOS, resolved through the
selected compiler, because its TLS implementation requires that runtime when Rust
links the C object. Clang uses the platform TLS runtime. These tests check native
C TLS behavior through generated wrapper bindings; they do not execute a Toucan
code generator. The storage rules follow
[C11 sections 6.2.4, 6.7.1, 6.7.6.2 and 6.7.9](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf)
and [GCC's TLS extension](https://gcc.gnu.org/onlinedocs/gcc/Thread-Local.html).


### C11 atomic types

The semantic library retains `_Atomic(T)` as a distinct object type, including
outer qualifiers, typedefs, and variable-array bounds inside atomic pointers.
Linux profiles use GNU layout; Darwin and Microsoft profiles use Clang layout.
For example, an atomic three-byte record occupies three bytes under GNU and four
under Clang. The underlying record retains its original layout.

Atomic lvalue conversions are explicit `AtomicLoad` steps. Stores and compound
updates have an `AtomicAccess` fact with sequentially consistent C semantics;
`ReadModifyWrite` describes one update rather than a separate load and store.
Initializers preserve the atomic destination and describe initialization of its
ordinary value, without implying an atomic store. Discarded atomic reads remain
observable. Unevaluated expression contexts still govern whether access occurs.

Compiler differences remain explicit: Clang retains atomic cast and function-call
rvalues, rejects aggregate brace initializers and atomic-pointer compound
assignments, and ignores atomic qualifiers inside array parameter brackets.
GNU permits incomplete atomic types behind pointers. Atomic values follow the
same typedef-ancestry rules and remaining arithmetic-alignment limits as ordinary
values, preserving observable `typeof` alignment. Direct atomic aggregate member access is diagnosed as undefined; load a complete ordinary value first.

Selected unqualified, naturally aligned atomic integers, Boolean values and
mutable object pointers use `core::sync::atomic` storage with generated size and
alignment assertions. Atomic enums use their compatible integer storage even
when `rustified_enums` is enabled, so values outside the named enumerators remain
representable. Function parameters and results use ordinary scalar carriers,
including nested callbacks; a pointer to an atomic object still points to atomic
storage. Clang atomic Boolean and integer values narrower than 32 bits are
rejected at call boundaries, including callbacks: their ABI differs from the
ordinary Rust scalar carrier. GNU profiles retain narrow scalar calls. Altered
scalar alignment and 128-bit atomic values also require a separate call-ABI
proof and are currently diagnosed. Their opaque object storage can still
be used through C pointers when its layout is representable.

Atomic floats, records, qualified objects and other storage without a compatible
core atomic use private `UnsafeCell<MaybeUninit<[u8; N]>>` storage. Its `uninit()`
constructor allocates storage only: a C initializer must establish the value
before C reads it. The wrapper exposes no safe value access and is neither
`Copy` nor automatically `Send`/`Sync`. Const and volatile atomic views retain
separate opaque representations. Use C accessors for these objects.

Records containing atomic storage omit `Copy`/`Clone`; union fields use
`ManuallyDrop`. Packed atomic containment and sizes incompatible with Rust
alignment are diagnosed. Atomic aggregates and records containing atomics cannot
cross calls by value: C conventions can differ from ordinary records with the
same size and alignment. C pointer accessors remain usable.

[Generated atomic binding evidence](../corpus/evidence/atomic-bindings-2026-09-08.json)
records actual GCC/Clang C↔Rust calls at O0/O2 on x86_64 Linux with Rust 1.64 and
current Rust, including callbacks and a shared counter. Generated Rust 1.64
layouts compile for all five targets; native calls on other targets are covered
by the ignored tests when CI runs them. These checks do not prove every C/Rust
concurrency interaction, atomic aggregate call ABI, or lock-free implementation.
Atomic type collection is bounded, and selections without atomics allocate no
atomic registry or containment cache.

[Atomic call-ABI regression evidence](../corpus/evidence/atomic-call-abi-2026-09-08.json)
records a native Clang/Rust counterexample: an atomic signed byte containing `-1`
reaches an optimized Rust callback as `255`. The failure also occurs for narrow
signed callbacks on Apple ARM. A one-field `repr(C)` wrapper changes ARM stack
argument slots, so equal storage layout does not repair the call boundary.
The updated native oracle selects the compiler profile explicitly, tests C
O0/O2 against Rust O0/O3, and checks every signed and unsigned 8/16-bit value on
the accepted GNU path. Clang storage and wider scalar calls remain exercised.
The C object is linked through a static archive so its runtime dependencies
precede the platform libraries. Native ARM and macOS runs remain CI gates;
the recorded local executions are x86_64 Linux.

[Atomic type evidence](../corpus/evidence/atomic-types-2026-09-08.json) records
compiler layout and constraint probes, native operations, retained graph checks,
and non-atomic allocation comparison. Layout does not establish lock freedom or
implement a concurrent memory model.

Exact atomic rvalue assignment and initialization preserve the atomic type in
Clang profiles, including pointers and aggregates. These value copies add no
atomic-load conversion. Clang still rejects an atomic pointer rvalue where an
ordinary pointer or a differently qualified atomic pointer is required; GNU
profiles apply their existing atomic-to-value conversion. The
[copy evidence](../corpus/evidence/atomic-copies-2026-09-08.json) includes native
compiler and retained-graph checks.

## AArch64 vector declarations

The GNU AArch64 profile recognizes `__Float32x4_t` and `__Float64x2_t` as
16-byte NEON types with 16-byte alignment. `VectorKind::Neon` preserves their C
type identity, which differs from an equal-shaped GNU `vector_size` type.
Assignments and arithmetic between these two forms preserve lane values;
pointer compatibility still distinguishes them. Arithmetic keeps the left
vector's identity, and comparisons produce GNU integer-vector masks. Clang
profiles do not expose these GCC-specific typedef names.

Both AArch64 profiles recognize `__SVFloat32_t`, `__SVFloat64_t`, and
`__SVBool_t`. `TypeKind::Sve` records their sizeless identity. Pointers have
normal target layout, but the SVE values themselves have no fixed size or
alignment. Arrays, record members, objects with static or thread storage,
`sizeof`, `_Alignof`, and pointer arithmetic cannot use a sizeless element.
Prototype parameters/results, pointer declarations, and type-only `_Generic`,
`typeof`, and fixed-size `sizeof` uses are supported. These rules follow the
[Arm C Language Extensions](https://arm-software.github.io/acle/main/acle.html).

`aarch64_vector_pcs` is retained as a function calling convention. Clang's
`aarch64_sve_pcs` is also retained; the GNU profile follows GCC 13's ignored
attribute behavior. `FunctionType::aarch64_pcs` reports register-preservation
rules, including the SVE convention implied by by-value SVE parameters or a
return value. This derived convention does not erase the written function type.
See the [Clang attribute reference](https://clang.llvm.org/docs/AttributeReference.html#aarch64-vector-pcs).

Evaluated SVE values currently require unsupported target-feature configuration.
Analysis reports that limitation before returning a successful result. Feature
requirements are discarded in proven unevaluated operands and constant dead
`if`, conditional, and short-circuit branches without labels. VLA bounds that
must execute keep their requirements. This is not a general reachability pass:
dead loop bodies and uncertain Clang numeric/object-size query operands can still
receive an unsupported-feature diagnostic. GCC permits replacing its predefined
vector typedefs; Toucan explicitly rejects that extension to preserve existing
alias identities. Ordinary block-scope shadowing remains supported.

Selected Rust bindings reject SVE types, including pointers: stable Rust cannot
express their sizeless representation. Explicit vector/SVE procedure-call
conventions also require C wrapper functions. NEON pointer storage uses the
existing aligned vector representation, while by-value vector FFI remains
unsupported. No SVE execution or AArch64 C-to-Rust runtime equivalence is claimed
by this layer. The [validation record](../corpus/evidence/arm-vector-types-2026-09-08.json)
includes cross-compiler probes and the unchanged ARM SQLite translation unit.

### Boolean macro constants

The default C macro policy emits actual `_Bool` constants as Rust `bool`.
Comparisons, logical operators, promoted conditional expressions and enum aliases
keep their C integer types. `--macro-type unsigned` and per-name unsigned policies
still emit the documented unsigned integer widths; a per-name C policy restores
Boolean output. Normalization reports retain the original C width and signedness.
Invalid public Boolean metadata is rejected before applying an integer override.

The [Boolean macro evidence](../corpus/evidence/bool-macros-2026-09-08.json) compares
generated consumers with independent GCC/Clang value, size and `_Generic` type
probes, including Rust 1.64. Packed enum constants initialized from Boolean
expressions retain C `int` type in macro aliases.

### Native atomic headers and Clang intrinsics

The Clang profiles check the `__c11_atomic_*` operations used by Clang's
`stdatomic.h`, plus its fetch-nand/min/max extensions. `C11AtomicOperation`
distinguishes initialization, loads, stores, exchanges, weak/strong
compare-exchange, fetch operations, fences, and lock-free queries. Parameter
conversions and memory-order operands remain explicit. Compare-exchange retains
an ordinary expected-value pointer, which is written on failure. Invalid orders
and incompatible pointer/qualifier constraints produce diagnostics.

C11 pointer fetch-add/sub operands count elements; GNU `__atomic` pointer operands
count bytes. C11 result type uses preserve the address operand's VLA bound identity.
Initialization does not imply an atomic store. The C11 lock-free query evaluates
its size operand and folds only zero and the guaranteed scalar sizes 1, 2, 4, and
8 to true. Other sizes remain runtime/target-dependent rather than becoming false.
Clang's function-pointer atomic arithmetic extension is explicitly unsupported.

Predefined character, least/fast integer, and scalar lock-free macros support the
native atomic typedefs. GNU fast16/32 types use `long`; Clang uses `short`/`int`.
The unchanged Clang resource header and its standard operations are checked on
Darwin and Microsoft targets with an explicit freestanding configuration. GNU's
header declarations are checked; its load/store macros still require the separate
`__auto_type` implementation. Linux Clang needs an independent compiler-profile
choice before its resource header can be used with the correct atomic layout.

[Atomic header evidence](../corpus/evidence/stdatomic-2026-09-08.json) includes
compiler predefines, cross-target type constraints, native operations, retained
VLA identities, and ordinary-path allocation comparison. It also preserves two
Clang 18 native failures: failed compare-exchange omits expected-buffer copyback
for a three-byte record, and pointer-to-VLA fetch-add uses a zero stride. The
positive runtime checks cover three-byte record load/exchange, four-byte record
compare-exchange, fixed-size pointer arithmetic, and VLA initialization/load;
they do not claim those compiler failures pass or validate a Toucan code generator.


### Packed enums

GNU `packed` enum tags select the smallest compatible integer that represents
all enumerators. Linux and Darwin profiles support signed and unsigned byte and
short representations; the Microsoft ABI ignores enum packing and keeps its
ordinary `int` representation. `TranslationUnit::enum_integer_kind` returns the
compatible integer without erasing the enum's nominal identity. Small enumerator
identifiers still have C `int` type, and narrow enum values promote to `int` in
arithmetic and variadic arguments.

Tag attributes apply at the definition. Clang also retains attributes on a
forward tag; GNU ignores them there. Packing a typedef or object does not change
its enum tag. Direct enum alignment attributes remain unsupported. GNU
`__alignof__(expression)` is a separate parser gap; type alignment queries work.

Default bindings preserve the compatible integer domain. Optional Rust enums use
the corresponding integer representation and require named valid variants.
Packed atomic enum storage uses the matching core integer atomic; the existing
Clang narrow atomic call diagnostic applies to its reduced width.

[Packed enum evidence](../corpus/evidence/packed-enums-2026-09-08.json) records
seven-profile type/layout comparisons and declaration constraints, generated
GCC/Clang C↔Rust calls on x86_64 Linux at independent C O0/O2 and Rust O0/O3,
Rust 1.64/current consumers, and unchanged default allocations and bindings.
The native tests cover callbacks, stack arguments, structs, unions and variadic
promotion. Cross-target compilation establishes layout/type constraints; native
macOS and ARM calls remain CI gates.

### Language modes

[C90, GNU90, C11, and GNU11](language-modes.md) select syntax, declaration rules,
standard-mode macros, and trigraph defaults independently of the compiler family.
GNU11 is the default. C90 supports implicit-int declarations and ordinary
implicit function declarations, including retained scope and call records.
Implicit library builtin signatures, inline-definition ownership, and GNU
omitted-middle conditional expressions remain tracked gaps. Selecting an ISO
mode does not enable pedantic diagnostics.

### Clang `noescape` parameters

Pointer-parameter promises participate in exact type identity and effective
function types while preserving ordinary C compatibility. Written annotations,
visible-redeclaration intersections, declaration-time call types, and K&R entry
contracts are retained separately. Binding signatures carry callback safety
comments. See [the contract model and validation scope](noescape.md).

### Typedef ancestry in integer expressions

Clang integer arithmetic and elementwise operations preserve the common typedef
ancestry that determines observable result alignment. The owned type API retains
alignment snapshots and canonical redeclaration links without enlarging `Type`.
See [typedef alignment](type-alignment.md) for supported conversions, metadata,
resource limits, and the remaining GNU/floating/pointer-composite boundaries.

### C11 non-return promises

`_Noreturn` now requires a function declaration and is retained independently of
function type compatibility. Ordinary declarations, retained declaration sites,
entities, and calls expose their applicable non-return promise; written source
spans survive macro expansion. Late declarations preserve earlier call facts.
Generated bindings keep the written C return type. See
[non-returning declarations](noreturn.md) for lexical inheritance and the
remaining control-flow diagnostic limits.
