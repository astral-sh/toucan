# Half and bfloat types

Toucan checks `_Float16` and `__bf16` declarations on supported targets other
than i686 GNU Linux. They are distinct C types with two-byte size and alignment.
Arrays, records, atomic storage and vectors preserve that identity; fixed
vectors remain limited to 16 bytes. `_Float16` uses IEEE binary16 (11
significant bits), while `__bf16` uses bfloat16 (8 significant bits and the
exponent range of binary32).

`FloatKind::FLOAT16` names the existing
`Extended { format: BinaryInterchange, width: 16 }` representation.
`FloatKind::BFloat16` identifies bfloat16. Encoded arithmetic constants use
`FloatingFormat::Binary16` or `FloatingFormat::BFloat16` and expose their low 16
bits without host floating-point conversion.

## Conversions and precision

Nominal arithmetic types follow the compiler probes: `_Float16` is the common
type when combined with `__bf16`; a standard `float`, `double`, or `long double`
operand selects that wider type. Neither narrow type undergoes the variadic
promotion from `float` to `double`. Checked expressions preserve those C types
and conversions; they are not an instruction-level evaluation plan.

The existing software APFloat evaluator supplies exact literal and cast rounding,
signed zeros, subnormals and non-finite encodings. Floating-to-integer conversions
retain range diagnostics. Finite overflow is diagnosed under the same policy as
other supported floating types.

GCC and Clang differ in constant evaluation. GCC 13 retains excess precision in
narrow arithmetic and even inexact `f16` literals before a wider conversion;
Clang 18's constant evaluator rounds the tested operations in their nominal type.
For example, `(float)1.00048828125f16` differs between them, while an explicit
`(float)(_Float16)1.00048828125` rounds on both. Toucan implements the verified
Clang constant behavior. GNU narrow binary/conditional constants and inexact
`f16` literal evaluation currently produce an explicit unsupported excess-precision
diagnostic. Exactly representable literals and explicit casts from supported
wider constants are available on both profiles.

This is separate from runtime arithmetic: Clang can retain binary32 intermediate
precision when implementing narrow operations. Its documented evaluation options
and the saved LLVM IR make that distinction explicit.
[Clang 18 half-precision semantics](https://releases.llvm.org/18.1.8/tools/clang/docs/LanguageExtensions.html#half-precision-floating-point).

## Rust bindings and remaining limits

Narrow scalar bindings, scalar pointers, and scalar call ABIs are explicitly
unsupported on stable Rust. Pointer-based vector storage uses the existing aligned
byte representation; vectors remain rejected in by-value calls. Narrow atomic
objects use the separate opaque atomic-storage representation where supported.

A macro explicitly cast to `float` or `double` can use the existing exact-bit
emitter. A narrow result is reported as omitted with its C type and a suggestion
to cast; it is never silently emitted as `u16` or `f32`.

ARM `__fp16`, GCC's `bf16` literal suffix, other extended floating formats, and
full GNU excess-precision constant evaluation remain separate work. The source
`f16` suffix is parsed; GCC and Clang reject it on i686 GNU Linux, where Toucan
also rejects it in expressions and macro values. Elsewhere evaluation is subject
to the GNU limit above. No optional instruction set or compiler evaluation flag
is implicitly enabled.

The [half-type evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/half-types-2026-09-08.json) records
compiler constraints, native object-byte comparisons, Rust boundary checks,
source/binary hashes, and the next unchanged Clang zstd header blocker. Successful
type checking is distinct from runtime code-generation and full conformance.

The [GNU AArch64 supplement](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/half-aarch64-confirmation-2026-09-08.json)
checks the actual GCC 13 cross compiler's target macros and emitted ARM assembly
for bfloat arithmetic and conversions. GCC 13 removed the earlier storage-only
restrictions and supplies software conversion helpers where needed. This is
cross-compiler syntax/code-generation evidence, without a native ARM execution claim.
