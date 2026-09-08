# GNU interchange and extended floating types

GNU `_Float32`, `_Float64`, `_Float32x`, and `_Float64x` retain distinct C
identities. Matching storage does not make them compatible with `float`, `double`,
`long double`, or each other. The parser also preserves Clang's ordinary typedef
and identifier uses of these names. Clang 18 does not provide these four builtin
types or their literal suffixes.

| C type | GNU x86-64 storage | GNU AArch64 storage | Rust scalar carrier |
| --- | --- | --- | --- |
| `_Float32` | IEEE binary32 | IEEE binary32 | `f32` |
| `_Float64` | IEEE binary64 | IEEE binary64 | `f64` |
| `_Float32x` | IEEE binary64 | IEEE binary64 | `f64` |
| `_Float64x` | x87, 16-byte object | IEEE binary128 | Unsupported |

The evaluator uses the target format and retains the C identity, signed zero,
and exact bits. At equal precision, GNU arithmetic prefers an interchange type,
then a standard type, then an extended type. For example, `double + _Float64`
has type `_Float64`, while `double + _Float32x` has type `double`.

These types do not undergo the default `float`-to-`double` argument promotion.
Retained call operands preserve that distinction. Stable Rust rejects `f32`
variadic arguments; replacing a C `_Float32` payload with `f64` would change its
contract. Use a C wrapper when passing `_Float32` through a variadic interface.
Fixed arguments, return values, callbacks, and supported records use the scalar
carriers above. Complex and vector call restrictions remain in effect. Atomic
interchange/extended floating calls require a separate ABI proof and produce an
explicit binding diagnostic; their opaque object storage remains available.

GNU complex and fixed-vector source types retain their corresponding real type.
Vectors with `_Float32` elements remain incompatible with vectors of `float`.
Wider variable scalars cannot be broadcast into narrower floating lanes. Existing
limits on vector width, excess-precision half evaluation, and complex operations
still apply. GNU arithmetic on altered-alignment floating typedefs also retains
the existing explicit unsupported diagnostic; it does not silently erase their
observable result alignment.

The default GNU version markers now select glibc's distinct builtin types.
Callers overriding the version macros for older header branches can still use
the legacy typedef spellings. This is an intentional compatibility exception:
GCC 13 itself reserves these names and rejects such replacement typedefs.
[Version-profile validation](compiler-version-predefines.md) exercises the
default header route and generated calls to the installed glibc.

The rules follow [GCC 13's additional floating types](https://gcc.gnu.org/onlinedocs/gcc-13.3.0/gcc/Floating-Types.html)
and its [arithmetic and argument conversions](https://github.com/gcc-mirror/gcc/blob/releases/gcc-13.3.0/gcc/c/c-typeck.cc).
Native tests compare object bytes, source constraints, and generated C/Rust calls.
Cross-compiled assembly supports the AArch64 ABI review; it is not native execution
on this x86-64 host. Unsupported `_FloatN` mathematical builtin families remain
separate features.

The [validation record](../corpus/evidence/gnu-float-types-2026-09-08.json.gz) is
compressed JSON containing source/binary hashes, compiler commands, native calls,
constant bytes, graph comparisons, and allocation measurements.
