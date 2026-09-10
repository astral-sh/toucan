# Compatibility

Toucan is an experimental C frontend and Rust binding generator. It implements
common C declarations and selected compiler extensions; it is not a complete C
compiler or a general drop-in replacement for bindgen. Rust output can have
stricter representation limits than the frontend's C type system.

## Targets

The canonical triples below are accepted by `--target`. A profile selects a
compiler family and language mode independently of the host running Toucan.

| Target | Compiler profiles | Important configuration |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | GCC, Clang | Supply GNU/Linux headers and compiler resources. |
| `aarch64-unknown-linux-gnu` | GCC, Clang | Supply AArch64 GNU/Linux headers. |
| `i686-unknown-linux-gnu` | GCC, Clang | 32-bit data model; nondefault 32-bit calling conventions are unsupported. |
| `armv7-unknown-linux-gnueabihf` | Clang | Hard-float ABI; generated Rust requires 1.78 or newer. |
| `x86_64-unknown-linux-musl` | GCC, Clang | Supply musl headers, not glibc. |
| `aarch64-unknown-linux-musl` | GCC, Clang | Supply AArch64 musl headers. |
| `x86_64-apple-darwin` | Clang | Supply the macOS SDK for the destination architecture. |
| `aarch64-apple-darwin` | Clang | Supply the macOS SDK. |
| `x86_64-pc-windows-msvc` | Clang | Microsoft ABI; supply Windows SDK headers. |
| `aarch64-pc-windows-msvc` | Clang | Windows ARM64 ABI and SDK. |

The default is GCC on Linux except ARMv7, and Clang elsewhere. C90, C99, C11,
and C17 each have ISO and GNU modes; GNU11 is the default. C23 and C++ are not
supported. Version predefines select specific header branches without promising
every feature of that compiler release. Flags such as `-fshort-enums`, optional
instruction sets, and arbitrary compiler versions are not implied.

See [compiler profiles](compiler-profiles.md), [language modes](language-modes.md),
and [header setup](usage.md#system-headers-and-cross-compilation). Acceptance of a
target is separate from native execution coverage. Use the maintained checks in
[validation](validation.md) on the revision and configuration being adopted.

## Supported C features

| Area | Scope and reference |
| --- | --- |
| Declarations | Scalars, pointers, arrays, typedefs, structs, unions, enums, prototypes, callbacks, variadics, and external objects. [Old-style definitions](old-style-definitions.md) retain incoming and local parameter types. |
| Names and scopes | Typedef/tag identity, compatible redeclarations, prototype and block scopes, and [anonymous record members](anonymous-record-members.md). Non-ASCII identifiers remain unsupported; `$` names are escaped in Rust. |
| Expressions and constants | C promotions/conversions, integer and target-format floating evaluation, casts, and [conditional expressions](conditional-expressions.md). A foldable query is not necessarily a valid C integer constant expression. |
| Initialization | Aggregate/designated initializers, static address values, strings, and compiler-specific [empty initializers](empty-scalar-initializers.md) and Clang [const-object values](const-object-initializers.md). |
| Variable-length arrays | Runtime bounds, nominal identities, and parameter adjustment; see [VLA identity](vla-type-identity.md) and [array qualifiers](array-typedef-qualifiers.md). |
| Layout | Target scalar models, record packing/alignment, enums, and bitfields. [Type alignment](type-alignment.md), [declaration alignment](declaration-alignment.md), and [enum alignment](enum-alignment.md) have separate rules. |
| Floating extensions | [Complex](complex-types.md), [GNU FloatN](gnu-float-types.md), [half/bfloat](half-types.md), and [binary128](binary128.md) have distinct nominal identities and target formats. Rust storage and call support is narrower. |
| Vectors | Fixed-vector expressions, [conversions](vector-conversions.md), [constants](vector-constants.md), and [shuffles](vector-shuffles.md). By-value Rust vector calls and SVE representations remain unsupported. |
| Intrinsics | Selected atomic, overflow, bit-count, allocation, object-size, x86, and AArch64 operations. [Feature queries](feature-queries.md) advertise implemented names; they do not bypass operand or target validation. |
| Function metadata | [Inline ownership](inline-functions.md), [target options](function-targets.md), [noescape](noescape.md), [noreturn](noreturn.md), and [DLL storage](msvc-dll-storage.md). |
| Function bodies | Calls, assignments, returns, control-flow constraints, supported GNU statement expressions, and inline-assembly operands. Retained code is optional; unsupported forms produce diagnostics. |

Ordinary and `u8` strings use UTF-8 code units. `u`, `U`, and `L` use the
selected target's UTF-16/UTF-32 representation. C11 escapes and adjacent literals
are supported; numeric escapes can preserve non-UTF-8 bytes. Alternative
execution character sets and C23 `u8` character constants are unsupported.

GNU `__int128` is available on supported 64-bit targets; i686 rejects it.
Generated 128-bit ABI types require Rust 1.78 with its bundled LLVM, and Rust
enums with 128-bit representations require Rust 1.89. Standalone integer
constants have a separate, older output-version floor. See [Rust versions](bindings.md#generated-rust-versions).

## Calling conventions

Ordinary calls use the selected target's C ABI. On x86-64, supported `ms_abi`
and `sysv_abi` declarations select Rust `win64`, `sysv64`, or the matching
platform-default `C` convention. Calling conventions also apply to nested
callbacks and function typedefs.

On i686, `cdecl` and the default SysV convention are supported; `stdcall`,
`fastcall`, `thiscall`, and `ms_abi` are rejected. AArch64 Linux/macOS reject
`ms_abi` and `sysv_abi`; Windows ARM64 follows its Clang Microsoft profile.
Vector/SVE procedure-call conventions require C wrappers. Nondefault-ABI
variadic traversal is not established by compiling a variadic declaration.

Transparent unions require a validated scalar parameter carrier. The carrier
is separate from ordinary union storage and return representation. Cases whose
padding requires unsupported expanded arguments are rejected. Matching record
size and alignment alone never establishes compatibility across an FFI call.

Windows x64 `__ptr32` retains four-byte C pointer storage, but generated bindings
reject it because native Rust pointers use eight bytes. The frontend can still
check unselected declarations containing these pointers.

## Record bindings

Ordinary records emit their actual fields with `repr(C)` and layout assertions.
Struct bitfields use byte storage and integer accessors; padding uses
`MaybeUninit`. Union bitfield accessors are unsafe and require initialized bytes.
Const bitfields have no setters; volatile bitfields require unsupported access
width and ordering semantics. Records containing bitfields cannot cross calls
by value.

Flexible and zero-length arrays support storage and pointer-based access.
Records containing them cannot cross FFI calls by value, including through
callbacks and enclosing records. Field-level alignment, combined packing and
explicit record alignment, and some empty-union call representations are also
unsupported. Use C accessors for affected APIs.

ARMv7 aggregate parameters are rejected when explicit alignment raises a
nonempty record with member alignments below eight bytes to at least eight
bytes. This includes enclosing aggregates and callbacks. Pointer storage and
compatible return-only declarations remain available.

## C11 atomic types

Atomic types have semantic identity and target layout separately from their
ordinary value types. Scalar bindings use compatible Rust atomic storage where
available; opaque cases require C accessors. Lock-free behavior is a separate
target/runtime property.

Clang's narrow atomic integer and boolean parameters/returns are rejected:
their extension rules can differ from Rust's ordinary scalar call ABI. GCC
scalar paths and wider Clang scalar paths retain their validated carriers.

Atomic floats, records, qualified views, and storage without a compatible core
atomic use private `UnsafeCell<MaybeUninit<[u8; N]>>` storage. `uninit()` allocates
storage only; a C initializer must establish a value before C reads it. These
wrappers expose no safe value access and are neither `Copy` nor automatically
`Send`/`Sync`.

Records containing atomic storage omit `Copy`/`Clone`; union fields use
`ManuallyDrop`. Packed atomic containment, incompatible alignment, and atomic
aggregate calls by value are diagnosed. C pointer accessors remain usable.
Layout and a successful binding generation do not prove every concurrency
interaction or provide a concurrent memory model.

## Current gaps

- The frontend is incomplete. Independent negative C coverage and broader
  compiler/target/header combinations remain necessary for general conformance.
- `long double`, complex, half/bfloat, and binary128 Rust representations have
  restrictions described in their feature references. A C layout does not imply
  a supported Rust storage or call representation.
- Direct thread-local, weak, and returns-twice bindings require representations
  or caller contracts that the generator cannot supply. Use C accessors/wrappers.
- Function-like macros are not translated into Rust functions. Object macros
  outside supported integer, float/double, and string constants are omitted and
  reported. SQLite destructor sentinels are examples; invalid function pointers
  are not synthesized for them.
- Unknown ABI attributes and unsupported selected representations are errors.
  An allowlist narrows generated output after analysis, not the C input checked.
- Bundled resource headers are limited fallbacks, not a C standard library or
  compiler SDK. Include paths and the correct target sysroot are caller inputs.
- Macro diagnostics locate the outer invocation without a full expansion trace.
- The public API and JSON schema are experimental. Successful layout or native
  checks on one configuration do not establish every consumer's Rust API needs.

## Rust consumer compatibility

The core generator and Builder have different defaults for macro types, enum
names, function definitions, and nullable function typedefs. Read the
[binding guide](bindings.md), [Builder API](../crates/toucan_bindgen/src/lib.rs),
and [macro compatibility](macro-value-compatibility.md) before replacing an
existing build script. Reports identify omissions; the CLI can reject omitted
macros with `--deny-skipped-macros`.

Use [replacement readiness](replacement-readiness.md) for consumer checks and
adoption gates. Historical captures are available through the
[validation archive](validation.md#historical-results), separate from this
current support contract.
