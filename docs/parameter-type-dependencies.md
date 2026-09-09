# Array parameter typedef dependencies

C adjusts an array parameter to a pointer. For example, `typedef int Array[4];`
followed by `void consume(Array);` has the same call signature as
`void consume(int *);`. Keeping only that checked signature loses the `Array`
declaration that bindgen includes when the function's header is selected.

The Builder now requests an optional `Analysis` catalog before adjustment.
The catalog records file-scope typedef names, their containing declaration or
record, and source locations. It also records dependencies of callback typedefs
and callback fields reached through other types. Binding generation follows these
edges without changing the adjusted C signature or array layout.

Core callers opt in with `AnalysisOptions::retain_parameter_type_dependencies`
and `BindingOptions::type_dependencies`. The default leaves the catalog absent.
The catalog does not require checked-code retention, and does not add fields to
`TranslationUnit`, `Type`, `Parameter`, or `FunctionType`.

## Selection and redeclarations

The policy follows bindgen 0.72.1's Clang source cursors:

- Direct function parameters use the first prototype's typedef dependencies.
- Nested callback parameters contribute dependencies at each selected occurrence.
- File selection can retain a signature's aliases even when the corresponding
  inline or internal function is omitted.
- Reachable callback typedefs and record fields retain their array aliases.
- `typeof` of a function designator retains the first signature's dependencies,
  using the declaration identity resolved by expression checking. Pointer-valued
  expression results do not add those dependencies. Local and parameter shadowing
  does not borrow an unrelated file-scope declaration.

Each occurrence has a source cursor and an outer owner source cursor. The latter
distinguishes compatible redeclarations of one entity for callback and name
selection. Locations use the normal preprocessed-source mapping; physical file
selection continues to ignore logical `#line` filenames.

Function types used only as expression operands do not create these dependency
edges. For example, `int count = sizeof(void (*)(Array)), other;` must not retain
`Array` through `other`. Capture is suspended during expression checking and
constant-expression classification, then restored for later declarators and
outer `typeof` function types. An actual record definition inside an expression
still records its callback fields, so reaching that record retains their aliases.

Capture allows at most one million copied references and 64 MiB of copied names
and source fragments, with a nesting limit of 128. Copies are charged before
allocation, including canonical signature copies and unsuccessful later work.
Caller-provided binding edges validate every owner and typedef name before use;
normal type collection deduplicates shared and cyclic dependencies.

## Evidence and remaining differences

The native study covers 99 accepted C fixtures with GCC 13.3 and Clang 18.1:
array chains, multidimensional arrays, function and callback redeclarations,
typedef and record roots, `typeof`, local shadowing, macros and physical files.
One standalone old-style definition remains unsupported by the Builder, as it was
before this change. Of the 98 previously supported cases, 96 have the same alias
set as bindgen; both remaining cases are `typeof` typedef chains.

Toucan preserves the intermediate `Callback` and `Chain` aliases in those two
cases, while bindgen drops them. Following their source dependencies can also
retain an array alias for a pointer-valued `typeof` chain. These are additive
declaration differences. This layer does not remove existing core aliases.

Other reference differences remain explicit: bindgen can select an earlier
nonprototype declaration and emit an incorrect zero-argument Rust signature.
Toucan retains the correct composite C signature. This catalog does not change
that behavior.

Focused tests cover optional capture, checked-code independence, unchanged C
types and diagnostics, source ownership, shadowing, declaration chronology, and
budget checks. Generated bindings were compiled with Rust 1.64 and the current
compiler, then called ordinary GCC- and Clang-compiled array, multidimensional
array, callback, and variadic functions in eight successful executions.

The [native and allocation capture](../corpus/evidence/parameter-type-dependencies-2026-09-09.json.gz)
preserves 1,103 source, command, diagnostic, output, and provenance artifacts.
Its implementation predates the expression-operand correction described above.
Default core allocation counts and generated output hashes are unchanged across
zlib, SQLite, zstd, and libgit2; requested bytes increase by eight per invocation
for the optional catalog pointer. The Builder requests the catalog and adds
48–307 allocations and 2,980–16,448 requested bytes across those headers. These
are allocation measurements with the system allocator, not latency measurements.

The [expression-operand correction capture](../corpus/evidence/parameter-expression-dependencies-2026-09-09.json.gz)
records eleven native controls and preserves the original 99 outcomes and all 98
supported generated outputs byte for byte. Four additional regression tests cover
expression boundaries, nested record definitions, and restored capture state.
Five checked-code seeds exercise catalog equality across 440 compiler-profile and
language-mode settings. Neither capture includes a mutation fuzzing campaign.
