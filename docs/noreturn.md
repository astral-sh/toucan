# Non-returning function declarations

C11 `_Noreturn` promises that a function does not return to its caller. It can
appear more than once and can be added in a later declaration. It requires a
function declaration: typedefs, objects, parameters, and empty declarations
cannot carry the specifier.

The promise does not change the C function type. In particular, taking the
address of `_Noreturn void stop(void)` still produces an ordinary function
pointer. Clang's GNU-spelled `noreturn` type contract remains a separate property
of `FunctionType`, including its existing pointer-conversion rules.

## Declaration and call facts

`Declaration::noreturn` describes the promise visible in the final file-scope
declaration. Retained analysis adds:

- `DeclarationSite::noreturn()` for the promise visible at that written declaration.
- `DeclarationSite::noreturn_source()` for its explicit specifier or attribute,
  mapped through preprocessing to the original source.
- `Entity::noreturn()` when any retained declaration carries the promise.
- `ExprKind::Call::noreturn` for a promise known from the callee's visible
  declaration or function type when that callee expression was checked.

The call fact is a snapshot. Later declarations, including declarations inside
argument statement expressions, do not rewrite it. A false value does not prove
that a call returns. Calls through an ordinary pointer that has lost the known
function identity do not acquire a declaration promise.

Clang keeps a block-local declaration's promise local when a file declaration
already exists. If the first linked declaration occurs in a block, a later file
declaration can inherit that promise. GNU propagates the block declaration's
promise to the linked function. Sites and calls retain their earlier facts in
both profiles.

Backend optimization can use additional knowledge. GCC can apply a late promise
when lowering an earlier caller; Clang can use an already-created LLVM function's
attributes. The retained call flag is not a complete control-flow analysis or an
instruction to reproduce a particular optimizer's choices.

## Validation and bindings

The checker records the promise without proving termination or diagnosing every
body that could return. C11 specifies a recommended diagnostic for such bodies;
a runtime return violates the promise. The existing unsupported diagnostic for
combining `returns_twice` and `noreturn` remains.

Rust bindings keep the function's written C return type. `_Noreturn int stop(int)`
continues to return the C integer type in the generated declaration. This metadata
does not introduce a Rust `!` return type or change the calling convention.

Ordinary analysis uses sparse promise state only when a declaration introduces
it. Retained source spans and name snapshots are charged to the existing graph
budgets before allocation. The lexical registry is capped at 65,536 bindings;
function types and ordinary scope frames keep their existing sizes. Caller-built
units are checked for non-return metadata attached to non-function declarations.

The C11 rules are in
[WG14 N1570, section 6.7.4](https://www.open-std.org/jtc1/sc22/wg14/www/docs/n1570.pdf).
Compiler probes distinguish source acceptance, declaration visibility, and LLVM
or assembly output; no machine-code generation by Toucan is claimed.

The [validation record](../corpus/evidence/noreturn-2026-09-08.json) includes
135 compiler comparisons, 2,552 ordinary/retained analysis pairs, unchanged zstd
inputs, and allocation measurements against the preceding implementation.
