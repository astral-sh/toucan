# Microsoft declaration attributes

The Clang MSVC profile parses `__declspec` and `_declspec` as language syntax.
Other profiles leave these spellings available as identifiers. Names and operands
remain in the syntax tree, including attributes between `struct` or `enum` and
its name. Commas and whitespace can separate attributes; empty attribute lists
are accepted. These keywords do not appear as replacement macros.

## Checked attributes

| Attribute | Behavior |
| --- | --- |
| `align(n)` | Requires one integer constant, a power of two from 1 through 8192 bytes. Retains record, typedef, field, parameter, function, or object alignment according to its placement. |
| `noreturn` | Takes no arguments on effective declaration subjects. Retains the written function promise and Clang's function-type promise, including through function-pointer typedefs. |
| `noinline` | Retains a function declaration's no-inline option and checks its argument count there. Other declaration subjects ignore the option, as Clang does. |
| `deprecated` | Accepts zero arguments or one string literal. Use-site deprecation diagnostics are not implemented. |

Narrow string annotation names used by SAL are retained in the syntax tree and
ignored, as in Clang C. Their text does not produce an analysis contract.
Recognized attribute operands still undergo ordinary name lookup even when their
declaration subject makes the attribute ineffective. SAL operands remain opaque. GNU-style surrounding underscores are
not aliases for Microsoft attribute names.

`has_declspec_attribute` reports checked support for `align`, `noreturn`, and
`noinline` in the MSVC profile. It returns zero for deprecation and string
annotations. It does not assert support for every vendor extension or target ABI.

## Placement and alignment

A prefix on a new record definition changes the record's layout:

```c
__declspec(align(16)) struct S { int x; } object;
```

Here `sizeof(struct S)` and its alignment are both 16. A prefix on an object
using an already defined `struct S`, or an attribute after the closing brace,
aligns that object without changing the record. Prefix alignment on a standalone
forward tag declaration is retained for its later definition. A pointer typedef
that merely introduces an incomplete tag instead aligns the pointer typedef.

Object and parameter metadata keeps Microsoft alignment separate from GNU
`aligned` and C11 `_Alignas`. `DeclarationAlignment::msvc` exposes the written
Microsoft value. Typedef alignment remains distinct from the underlying type's
size; for example, an aligned `int` typedef can have size 4 and alignment 16.
MSVC field-layout rules preserve that required alignment inside packed records.
Decreasing a typedef's alignment does not reduce an MSVC record field's natural
alignment.

A prefix `noreturn` on a function returning a callback applies to the outer
function. Strengthening an ordinary function pointer to a `noreturn` pointer
is rejected; weakening that promise is permitted. Generated callback types retain
the existing C promise comment, without changing the physical calling convention.

## Current limits and evidence

Enum-tag alignment and alignment on a void or function typedef retain explicit
unsupported-feature diagnostics. DLL import/export, thread storage, `selectany`,
and additional Microsoft attributes are also unsupported in this layer.
These diagnostics are not compiler acceptance equivalence claims. Full Windows
SDK parsing and native Windows runtime validation remain release gates.

The focused Clang tests cover source acceptance, placement, argument constraints,
name lookup, function-pointer conversions, and C static assertions for record,
field, object, and typedef layout. Every fixture also compares ordinary analysis
with retained analysis. Parser tests check original spans, tag placement, operand
traversal, deterministic work limits, and disabled-profile identifier behavior.

The [saved evidence](../corpus/evidence/declspec-2026-09-08/summary.json) records
Clang probes, retained-graph checks, the eight explicit enum-alignment limitations,
unchanged existing binding outputs, and generated Windows output compiled with
Rust 1.64 and current Rust. These are cross-target compiler checks, not native
Windows execution or a performance measurement.
