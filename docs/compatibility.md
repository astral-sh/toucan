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

## GNU statement expressions

Statement expressions (`({ ...; expression; })`) check their local declarations,
expressions, and control flow within the enclosing function. Jumps into these
blocks and jumps into VLA scopes are rejected. Their result categories and loop
condition behavior follow the target's GCC or Clang profile. Constant evaluation
of statement-expression bodies and GCC's precise-width bitfield result types
remain unsupported and produce diagnostics.

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
- Rust bindings reject union bitfields, records containing bitfields passed by value,
  field-level alignment, and combined packing and explicit record alignment.
- Function-like macros and object macros that are not supported integer or
  string constants are reported as omitted. SQLite's `SQLITE_STATIC` and
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
