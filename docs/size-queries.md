# Void and function size queries

GNU and Clang profiles accept `sizeof(void)` and `sizeof` applied to a function
type or designator. The result is 1 with the target's `size_t` type. This includes
void expressions, such as a void function call, a cast to void, or an indirection
through a void pointer. The operand stays unevaluated.

These are compiler extensions, also accepted by GCC and Clang in their ordinary
`c90` and `c11` modes. A native `-pedantic-errors` invocation rejects them. Toucan's
language modes select compiler semantics; they do not enable a pedantic diagnostic
policy. [GCC documents the size extension](https://gcc.gnu.org/onlinedocs/gcc/Pointer-Arith.html).

Void and function types still have no object layout. `TranslationUnit::layout`
rejects them, and the size extension does not permit void objects, arrays of
functions, incomplete object queries, bitfield queries, or pointer arithmetic on
void/function pointers.

## Alignment and typedefs

`_Alignof`, `__alignof`, and `__alignof__` use the existing
[expression alignment rules](expression-alignment.md). Unannotated void alignment
is 1. Function alignment is 1 for GNU x86-64 profiles and 4 for the other supported
profiles. GCC ignores GNU-spelled alignment on void/function typedefs; Clang
preserves it for type queries. Function declaration alignment is separate from
typedef alignment. These annotations do not change the storage or call ABI of a
pointer to the annotated type.

Bindings therefore emit ordinary opaque pointers and callbacks. C size and
alignment macro values remain C query results; Rust's `size_of` for a function
pointer is the pointer size. Microsoft `__declspec(align)` on void/function
typedefs remains explicitly unsupported.

## Retained expressions and validation

The checked graph keeps size operands unevaluated and preserves their written
types. GNU void lvalues have `ValueCategory::ObjectLvalue` with a void type and no
lvalue-to-value load conversion. Discarding such an expression does not create an
object value.

The tests compare query values across all compiler profiles and four language
modes, preserve rejection controls for incomplete objects and pointer arithmetic,
and run native side-effect checks. Native C/Rust consumers exercise aligned void
aliases, records containing pointers, and callbacks in both directions with
current Rust and Rust 1.64.
