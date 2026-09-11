# Identifier-list function definitions

Toucan checks C11's identifier-list (K&R) definitions:

```c
double measure(count, value)
    unsigned char count;
    float value;
{
    return (double)count + value;
}
```

A call without a prototype passes promoted arguments: `int` for `count` and
`double` for `value`. Function entry converts them to the declared local types.
The definition does not supply a prototype for later calls. An earlier compatible
prototype supplies the calling interface, including GNU/Clang's extension that
accepts a matching unpromoted parameter type or a variadic prototype.

Every identifier needs exactly one typed declaration. The checker rejects missing
or duplicate declarations, extra names, initializers, invalid storage classes,
incomplete parameter types, and a declaration list following a prototype. Arrays
and functions adjust to pointers. `register`, qualifiers, tags, callbacks and
variable array bounds use the same checks as prototype parameters. GCC's implicit
`int` parameter extension remains outside the C11 contract.

Name resolution follows the written declaration order. For example, a bound in
`int array[n]; int n;` sees an outer `n`, if any. Completed parameters bind incoming
arguments in identifier-list order. Bounds and entry conversions finish before
the body; the checked API does not promise a total execution order between
different parameters.

Canonical non-prototype function types contain no parameter types, matching
`typeof(function)` and function-pointer compatibility. Direct redeclarations
also check a separate definition record, so an intervening `f()` cannot erase the
known parameter constraints. That record uses linked declaration identity and
retains the actual incoming types. GNU promotes the contained value of a narrow
atomic parameter (`_Atomic(char)` to `_Atomic(int)`, for example); Clang preserves
its complete atomic type. This rule does not introduce an atomic load or change
ordinary atomic argument checking. GNU alignment attributes on identifier-list
parameters follow the shared parameter rules: GCC rejects them; Clang retains their
explicit and effective storage alignment on the declaration site. They do not change
the parameter type, incoming argument type, or entry conversion. C11 `_Alignas`
remains invalid on parameters.

## Checked API

`FunctionBody::old_style()` supplies:

- Declaration groups in written source order.
- Parameter entries in identifier-list order, each linking its header occurrence,
  typed declaration site, incoming `TypeUseId`, and entry conversion steps.
- An explicit `UnspecifiedBetweenParameters` evaluation order.

`FunctionBody::parameters()` uses identifier-list order. Its `signature()` contains
adjusted local parameter types; `old_style()` gives incoming types, while the
definition's declaration site links the canonical calling interface. Declaration sites retain
the original pre-adjustment type and bound identities; entry conversions describe
C values rather than machine registers. An atomic incoming value is converted
without reading an atomic object. The optional fields extend the experimental
checked schema 4; ordinary definitions keep their existing serialized shape.

The parser and checked-graph limits apply. Additional definition metadata is
limited to 64 MiB, with 65,536 parameters per identifier list. The default path
allocates this metadata only for identifier-list definitions. Binding generation
continues to omit function definitions; non-prototype declarations and callback
types retain their explicit unsupported Rust-signature diagnostic.

## Compiler evidence

The [saved probes](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/old-style-definitions-2026-09-08.json) cover
GCC and Clang constraints, promotions, callbacks, VLA effects and native execution.
Seven-profile tests compare normal and retained results, original declaration
sites, inferred atomic incoming types, quota failures and C11's minimum parameter
count. Native failures are checked for compiler crashes separately from ordinary
constraint rejection.

The checker preserves C11 source semantics for isolated compiler defects:

- GCC 13 drops effects in an adjusted-away outer K&R array bound. Clang evaluates
  it, and both evaluate its prototype-definition equivalent. Toucan retains the
  bound's required entry effect.
- GCC 14.2 on Linux and Homebrew GCC 14.4 on macOS also drop bounds that remain
  in pointer-to-VLA parameters. The [recorded native discrepancy](compiler-oracle-discrepancies.md#gcc-14-old-style-parameter-bounds)
  preserves the failing results and matching prototype controls.
- Clang 18 discards declaration-list tags and enumerators before checking the body.
  C11 gives these declarations block scope; GCC and Toucan preserve that scope.
- GCC 13 can forget definition parameter constraints after an intervening empty
  declaration. Toucan continues to check later prototypes against the definition.

These observations do not change the compiler profile's language contract.
The relevant rules are C11 6.2.1, 6.7.6.2–3 and 6.9.1 in
[N1570](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf).
[WG14's rationale](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n881.pdf)
explains required VLA size-expression effects. The implementation also follows
[GCC 13.3 parameter declaration handling](https://github.com/gcc-mirror/gcc/blob/releases/gcc-13.3.0/gcc/c/c-decl.cc)
and [Clang 18.1.3 declaration merging](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaDecl.cpp).

Historical allocation, timing, and integration results remain in the
[validation archive](validation.md#historical-results). Run the native
`old_style_definitions` tests on the revision being adopted.
