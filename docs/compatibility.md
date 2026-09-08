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
pointer, and reject remaining runtime-sized array layers. Runtime bound expressions
and typed bodies are not yet exposed through the public IR.

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

Object-size inference and allocation-provenance tracking remain unsupported.
These query values can depend on optimization, and the dynamic form can require a
runtime size computation. Requests to evaluate an extent as a constant return a
diagnostic. Unknown extents are not replaced with a size or a sentinel in the
analysis result. See [GCC's object-size contract](https://gcc.gnu.org/onlinedocs/gcc/Object-Size-Checking.html).

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
| `x86_64-unknown-linux-gnu` | Implemented; GCC and Clang probes | [C/FFI and differential checks passed](../corpus/evidence/native-equivalence-2026-09-08.json) |
| `aarch64-unknown-linux-gnu` | Implemented; Clang cross-target probes | [C/FFI and differential checks passed](../corpus/evidence/native-equivalence-2026-09-08.json) |
| `x86_64-apple-darwin` | Implemented; Clang cross-target probes | [C/FFI and differential checks passed](../corpus/evidence/native-equivalence-2026-09-08.json) |
| `aarch64-apple-darwin` | Implemented; Clang cross-target probes | [C/FFI and differential checks passed](../corpus/evidence/native-equivalence-2026-09-08.json) |
| `x86_64-pc-windows-msvc` | Implemented; Clang cross-target probes | Not run; MSVC header syntax is incomplete |

The recorded [native run](https://github.com/astral-sh/toucan/actions/runs/34176518153)
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
reject those operations. Implicit conversions between different vector types,
enum lanes, non-power-of-two sizes, extended-vector swizzles, and architecture
intrinsics are outside this slice. Static vectors accept brace initializers and
vector compound literals; other static vector-expression evaluation is explicitly
unsupported.

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

## Current gaps

- Bodies and initializers are type-checked, including the supported GNU statement
  expressions and inline assembly operands. Unsupported constraints produce
  diagnostics. Typed bodies are not yet exposed through the public IR.
- Declaration constraints still need broader conformance testing. Prototype-local
  tags retain distinct identities and are checked against GCC and Clang.
- C++, K&R function definitions, TLS, atomic and complex
  types, unsupported calling conventions, and unknown ABI attributes are rejected.
- Extended floating-point types retain their identity but do not have supported
  layout or binding representations. `long double` has a target layout, but its Rust
  binding representation is not implemented.
- Large enum constants follow the target's GCC or Clang profile, including their
  types during and after the definition. Apple enum ranges requiring more than
  64 bits and values that cannot be represented without truncation are rejected.
- Rust bindings reject records containing bitfields passed by value, field-level
  alignment, and combined packing and explicit record alignment. Union bitfields
  have the pointer-based representation described below; volatile bitfields and
  bitfields with Rust enum representations remain unsupported.
- Function-like macros and object macros that are not supported integer, finite
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

## Rust consumer compatibility

ABI equivalence alone does not guarantee that existing Rust callers compile.
The optional Rust enum, `size_t`, and integer macro policies are tested with
unmodified `zstd` and `zstd-safe` wrappers using freshly generated bindings for
`zstd-sys`. The [consumer fixture](../tools/zstd_consumer/README.md) pins the
versions used by the inspected Ruff and uv checkouts and exercises bulk and
streaming compression. Its evidence is separate from the default-output bindgen
comparison below.

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

Complete the declaration and expression conformance work; replace or establish
reliable limits for pathological parser backtracking; extend the target/header
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

Arithmetic and conditional expressions involving an unpromoted typedef with a
changed alignment currently produce an explicit diagnostic. GCC and Clang retain
different typedef identities in these result types; Toucan does not discard that
observable `typeof` alignment. Combining pointer types with changed-alignment
pointees also produces an explicit diagnostic. Integer promotions and ordinary object, pointer,
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
