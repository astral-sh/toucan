# Conditional expressions

GNU `condition ?: fallback` evaluates the condition once and uses its saved value
when nonzero. The fallback runs only when the condition is zero. Both branches
still undergo type checking. Integer promotions, floating and complex conversions,
null pointer constants, pointer qualifiers, and VLA bounds follow the ordinary
conditional operator. The result is a value, so it cannot be assigned to or have
its address taken.

All modeled GCC and Clang profiles admit this extension in C90 through C17 and
their GNU modes. The standalone parser's strict standard flavor rejects it.
GCC 13 and Clang 18 reject omitted-middle syntax in `#if`, including a dead
short-circuit operand. Preprocessing keeps that distinction. A macro replacement
used in C source can contain the operator.

## Checked graph

The parser stores an absent middle operand without copying the condition AST.
Visitors traverse the written condition once. The checked graph uses
`ExprKind::OmittedConditional` with three uses:

- `condition` evaluates its expression and performs any load or array/function
  decay needed for the scalar condition.
- `then_value` has `UseContext::ReusedValue` and the same expression ID. It consumes
  the saved, converted condition. Its conversion list contains only further
  result conversions; it does not repeat a volatile read, atomic load, decay,
  function call, or increment.
- `else_value` evaluates the fallback when the condition is zero.

A graph consumer must preserve this use context instead of recursively evaluating
the referenced expression again. The saved value keeps the condition's converted
type-use provenance, including VLA bounds after array decay. Composite bounds
record the source condition as provenance; they do not promise that runtime
bounds from different operands are equal.

## Static pointer initializers

A bounded address walk distinguishes numeric pointer bits from symbolic addresses.
Numeric casts truncate to the target pointer width before testing for zero.
Symbolic constants retain their base, byte offset, and declaration-time weak
binding. The walk never reads an object or assigns a concrete address to a symbol.
It covers named objects and functions, strings, file-scope compound literals,
member and array addresses, pointer casts and offsets, and nested conditionals.
Scalar object reads and calls are not address constants.

| Initializer condition | GCC 13 | Clang 18 |
| --- | --- | --- |
| Strong object/function address | Select the nonzero branch | Select the nonzero branch |
| Numeric pointer with zero target bits | Select the fallback | Select the fallback |
| Prior weak address, strong fallback | Reject as nonconstant | Reject as nonconstant |
| Prior weak address, null fallback | Preserve the address expression | Reject as nonconstant |
| Address declared weak only afterward | Check using the earlier strong binding | Check using the earlier strong binding |

GCC also accepts the explicit identity `p ? p : 0` for a weak address constant.
The checker compares the symbolic base and byte offset before admitting that
case. It does not substitute a different address merely because both arms have
pointer types. A local static object shadowing a weak global uses its local
binding.

Native object-file probes preserve relocations and symbol tables alongside
executable checks. In particular, GCC can later mark a previously strong symbol
weak: an earlier initializer still contains that symbol's relocation, so an
absent definition can produce a null pointer even if the written fallback was
nonzero. Initializer checking does not retroactively fold from final binding
metadata. Clang ignores the corresponding late weak declaration in those probes.

These are initializer-admission rules. The public checked graph preserves source
expressions and declaration-site binding snapshots; it does not expose a folded
symbolic-pointer value. A constant-lowering consumer must use the declaration
state at the source position rather than the final entity binding.

## Limits and evidence

Saved conditions are reused by integer and arithmetic evaluation, static
initializer validation, and side-effect walks. Nested omitted operators do not
expand into duplicated operand trees. The address walk shares the existing
128-level expression limit. Retained uses, conversions, and type metadata remain
charged to the graph's node, edge, and payload budgets.

This change does not broaden general integer-constant-expression admission.
For example, GCC and Clang accept some floating comparisons in `_Static_assert`
as extensions; the existing integer evaluator rejects them for both ordinary and
omitted conditionals. Floating and complex values are available through arithmetic
constant evaluation and supported static initializers. Object-size and constant
knowledge queries retain their existing conservative folding rules.

The native syntax, relocation, runtime, generated Rust 1.64 bindings, corpus,
benchmark, and sanitizer records are archived in the
[evidence summary](../corpus/evidence/omitted-conditional-2026-09-08/summary.json).
The GCC [omitted operand documentation](https://gcc.gnu.org/onlinedocs/gcc/Conditionals.html)
defines saved-value behavior; the compiler-version-specific decisions above come
from executable probes.
