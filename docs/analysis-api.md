# Analysis API

Toucan checks supported declarations and function bodies on every analysis. Retaining
the checked graph is optional: binding-only consumers pay no graph allocation cost.
The API is experimental; it does not promise full C11 or compiler-extension coverage.

```rust
use toucan_semantic::{analyze_with_options, AnalysisOptions};
use toucan_target::Target;

fn main() -> Result<(), toucan_semantic::Error> {
    let analysis = analyze_with_options(
        "int twice(int x) { return x + x; }",
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions { retain_code: true, ..AnalysisOptions::default() },
    )?;
    let code = analysis.checked().unwrap();
    for (id, body) in code.bodies() {
        let function = code.entity(body.entity()).unwrap();
        let statement = code.statement(body.statement()).unwrap();
        println!("{id:?}: {:?}: {:?}", function.name(), statement.kind());
    }
    Ok(())
}
```

`Analysis` owns declarations and all retained payloads. Its input string can be
dropped after analysis. Getters return shared references, slices, or copied IDs;
there are no mutable arena accessors. `into_unit()` discards the retained graph and
returns owned declarations. IDs are local to an analysis. Lookup checks bounds,
but cannot detect an in-range ID taken from a different owner.

The graph retains:

- Entity identities, declaration sites, lexical scopes, and written references to
  typedefs, tags, fields, and ordinary names.
- Expression types and value categories, plus conversions at each operand use.
- Runtime array bounds and static parameter contracts attached to type uses;
  canonical types preserve the shape independently of an evaluated extent.
- Function bodies, statements, resolved labels and jump targets, assembly operands,
  and declaration groups.
- Initializer trees, braces, designated member paths, sparse ranges, string copy
  lengths and termination, implicit zero fill, and flexible-array storage.

The graph describes checked C syntax. Its operand and initializer ordering records
written structure and overrides; it does not choose an order for C side effects.
Successful results contain no unfinished nodes or missing expression, statement,
or initializer coverage. Attribute metadata and parser-inserted text have explicit
coverage classifications. Retention limitations produce diagnostics.

## Runtime types and initializer destinations

`type_use()` links expressions and declarations to a canonical type shape plus
runtime-bound metadata. A parameter's `declared_type_use()` preserves dimensions
and static minimum contracts before array-to-pointer adjustment. A bound records
its expression or composite inputs and its evaluation context; `Required` means
required when the containing construct is reached, including its enclosing control
flow. It does not mean unconditional execution.

An initializer's `ty()` is its completed destination shape. Its destination type
use is available from the owning declaration or compound-literal expression.
For nested initializers, follow their sparse subobject paths from that destination;
array index and range steps project an element. An assignment operand's converted
type use describes that operand, not a replacement for the destination's bounds.
For example, in `int a[n]; int (*p)[n] = &a;`, the two written `n` bounds have
separate identities. The declaration site for `p` retains its destination bound;
its initializer's `AssignmentId` links to the operand use and the bound of `a`.
Their canonical pointer-to-array shapes can be equal without merging those sites.

## Integrated preprocessing and provenance

Set `Config.analysis.retain_code = true` before `parse_file` or `parse_source`.
`Compilation::unit()`, `analysis()`, `checked()`, `preprocessed()`, and `timings()`
provide immutable views of the result. This replaces the former public fields.

A node's `occurrence()` leads to its `SourceSpan`. `range()` covers byte offsets in
the original preprocessed input; `fragments()` preserves disjoint or reordered
pieces when adapters transform syntax. A synthetic span has no written tokens.
`Compilation::source_locations(span)` resolves the intersecting token origins,
including included files and outer macro invocations. It may repeat an origin for
several expanded tokens and does not provide a full macro expansion backtrace.

## Resource limits

`AnalysisOptions::default()` disables retention. With retention enabled, `limits`
bounds logical nodes, graph references, and owned payload bytes. The defaults are
1,000,000 nodes, 4,000,000 references, and 128 MiB of charged payload. Allocator
overhead is not included. Existing parser, input-size, and semantic nesting limits
also apply. Exceeding a limit returns a diagnostic; no partial graph is returned.

## Written type ownership

A declaration's `TypeUse::functions()` links each written function declarator to
its prototype scope and parameter sites. `FunctionUse::path()` distinguishes
nested callbacks and return types. Definition scopes are promoted in place, so a
function body's parameters and its declaration's parameter links share identities.
Parameter sites retain both adjusted pointer types and written array minimums.
Typedefs reuse their original prototype links. Conditional function-pointer
expressions may retain multiple written origins at the same path; these do not
assert that their parameter contracts are identical.

GNU `typeof` operands have their own arena. Starting at a declaration site, follow
`occurrence()` → `Occurrence::type_owner()` → `Occurrence::type_operands()`.
Type owners also exist for declarations without a named declarator. Expressions
with written type names expose `Expression::type_name()`; its occurrence owns the
corresponding operands. Nested `typeof(type-name)` inputs link directly to that
type name and its type use.

Each operand records its lexical scope, checked input and evaluation context.
`Required` means required when execution reaches that owner; it does not bypass
conditional or short-circuit control. Prototype and unevaluated uses are explicit.
`MayBeOmitted` records noncontributing variably modified type operands within
`sizeof(type)`. For example, `typeof(p++)` with a pointer to a VLA can evaluate
`p++` while reusing an existing bound. A later use of a typedef shares the bound
and does not acquire the original declaration's operand execution.

[Ownership validation](../corpus/evidence/type-ownership-2026-09-08.json) records
native side-effect probes, external corpus parity and default allocation checks.

## Compiler query results

Object-size builtin calls expose an optional `ObjectSizeProof`. `whole_bytes()`
and `subobject_bytes()` describe structurally identified storage ranges. `result()`
separately reports a known scalar value and its fold stage, or an unresolved
compiler answer. A default sentinel is marked explicitly and is not a range proof.
For example, GCC can return different mode-three values for `record.buffer + 2`
across optimization levels; the graph can still retain the buffer's remaining
bytes without claiming a scalar result.

`QueryEvaluation` governs the first argument's execution. A frontend object-size
fold suppresses operand evaluation. A known later fold can still execute Clang's
conditional scalar fallback, including fresh VLA bounds. Follow the argument's
expression tree and its query policy; a known scalar value alone is not proof that
its operand is unevaluated. External folding APIs accept supported later facts,
while nested declarations, types and static initializers retain frontend constant
constraints.

## Functions that may return more than once

`Declaration::returns_twice` and `Entity::returns_twice()` preserve the merged GNU
function annotation. `DeclarationSite::returns_twice()` records the state when a
declaration was checked; `returns_twice_attribute()` gives its explicit original
source span. Follow the entity's sites and their scopes to distinguish earlier
prototypes from later annotations. These flags are not C type qualifiers and do
not claim that a compiler retroactively annotates earlier calls. Ordinary function
pointers retain their ordinary C types; indirect-call target analysis remains a
consumer responsibility. Binding generation rejects selected direct returns-twice
functions because Rust cannot express the required caller contract.

## Object storage duration

`Declaration::is_thread_local` distinguishes file-scope thread-local objects from
ordinary objects. TLS belongs to an object, not its C type. `Entity::storage()`
and `DeclarationSite::storage()` expose `Storage::Thread`, `Static`, `Automatic`,
or `None`; linkage remains a separate property. Block `extern` declarations share
their linked entity, while a block-static shadow has its own identity. Source
spelling remains available through the declaration's written type-owner occurrence.

`Initializer::requires_constant()` covers both static and thread storage. It
replaces the earlier `static_storage()` getter and serialized field. For example,
`_Thread_local const char *text = "hello";` has a constant initializer, while
`static int *p = &thread_object;` is invalid: a TLS address is computed for the
current thread. Runtime address-taking is valid and retains ordinary pointer types.

## Declaration origins without body retention

Set `AnalysisOptions::retain_declaration_origins` to retain written file-scope
declaration locations independently of `retain_code`. `Analysis::declaration_origins()`
(and the facade's `Compilation::declaration_origins()`) returns source-ordered
occurrences. Compatible redeclarations share their canonical target while keeping
separate locations and definition/linkage facts. Tentative object declarations
report `is_definition() == false`; the final declaration data reflects translation-unit
completion. Targets index the same owned
translation unit's declarations, records, enums, and enumerators. Tag references
are included with `is_reference() == true`; they are not independent file-selection
roots. Block-local declarations are not included.

This catalog does not allocate expression, statement, or initializer arenas and
does not consume checked-code budgets. It has separate fixed limits of one million
occurrences and 64 MiB of mapped source fragments. Origins are absent by default.
Their byte ranges refer to the original preprocessed input after parser adapter
mapping, just like checked-code spans.

For physical header selection, also enable
`Config::preprocessor.record_file_origins`. Resolve an origin's starting byte
through `compilation.preprocessed().file_origins().unwrap().source_file(offset)`.
These input paths stay separate from diagnostic `#line` paths. A declaration
expanded by a macro belongs to the file containing its invocation. The physical
macro-definition lookup reports the final active definition and removes names
after `#undef`; it does not implement a binding generator's macro-history policy.
