# Inspect checked C code

Use `inspect --checked-code` to export the information available to analyzers and
header indexes:

```console
toucan inspect api.h --target x86_64-unknown-linux-gnu --checked-code --output api.json
```

The command preprocesses and checks the entire input, including function bodies.
Its version 5 JSON contains:

- `translation_unit`: target and compiler identities, declarations, canonical types,
  records, and enumerations.
- `checked_code`: owned arenas for source occurrences, scopes, entities,
  declaration sites, expressions, bodies, initializers, and type uses.
- `preprocessed`: the exact source addressed by those occurrences and mappings to
  original file locations or macro invocations.

Without `--checked-code`, `inspect` uses the version 3 declaration-only format.
Both formats are experimental; consumers should check `schema_version` and tolerate
additive fields. The `compiler` field uses `gcc` or `clang`; see
[compiler profiles](compiler-profiles.md) for defaults and compatibility.

## Follow references

Arena IDs serialize as integer indices. For example, a reference's `target`
indexes `checked_code.entities`; a function entity's `body` indexes
`checked_code.bodies`. A body identifies its definition site, parameters, and root
statement. IDs belong to that one result and cannot be transferred to another run.
The Rust library provides typed IDs and checked lookups for the same data.

A declaration's effective type includes information contributed by compatible
redeclarations. Each initializer records its checked destination type. Aggregate
entries preserve written override order; C does not require side effects to run
in that order. Sparse ranges and implicit zero initialization are represented
without creating an entry for every element.

Runtime array dimensions belong to type uses. A typedef can reuse an earlier
bound evaluation, and array parameters retain their declared contracts before
adjustment to pointers. Evaluation metadata describes what happens when a
construct is reached; enclosing conditions and control flow still determine
whether execution reaches it.

Sparse function option and inline-target maps preserve per-declaration target settings,
written attribute spans, and compiler-specific inlining stages. They are omitted
when empty. See [per-function target options](function-targets.md).

## Resolve source locations

Source ranges use half-open UTF-8 byte offsets into `preprocessed.source`.
`fragments` records disjoint or reordered original pieces when a parser adapter
has rearranged syntax. Synthetic occurrences have no written source token.

The ordered `preprocessed.mappings` connect generated ranges to source anchors.
Locations use one-based lines and byte columns, including `#line` remapping.
A `macro_invocation` anchor identifies the outer invocation; the output does not
currently provide the nested expansion stack or macro definition location.

Semantic or retention-limit failures produce a diagnostic and preserve an existing
output file. Successful checked analysis has complete expression, statement, and
initializer coverage within the supported feature set. Resource limits still
apply; the library exposes them through `AnalysisOptions`.

## Migration from versions 1 and 2

Versions 3 (declarations) and 4 (checked code) add `identity` to the existing
`VariableArray` struct variant. Its element remains a `Type`; for example:

```json
{"VariableArray":{"element":{"alignment":null,"kind":{"Integer":"Int"},"qualifiers":{"is_const":false,"is_volatile":false,"is_restrict":false}},"identity":7}}
```

The identity is an opaque number local to one analysis result. It distinguishes
separately written runtime array types, including equal-looking bounds and written
prototype stars. Copies of one object or typedef type retain its identity. It is
independent of checked `BoundId`: a type identity does not schedule a bound
expression or determine a composite array's runtime size. Ordinary C compatibility
continues to use its existing VLA rules.

Consumers matching `TypeKind::VariableArray` in Rust must bind `identity` or use
`..`. JSON consumers should accept schema 3 for declarations and 5 for checked code, and read the field when exact
type identity matters. Identities are not interchangeable between independent
analysis results. Expression queries use a separate occurrence registry and reserve
identities beyond those inherited from their input translation unit.

See [VLA type identities](vla-type-identity.md) for construction limits and validation.

The sparse function options may include `minimum_vector_width` for a Clang
function declaration. This is the explicit hint in bits, not the backend's
computed vector width. Checked option sites additionally retain the ordered
written hints and their source spans. Absent hints omit these optional fields;
these fields remain available in declaration schema 3 and checked schema 5.

Checked expression output may include `ShuffleVector`: two value operands plus
either ordered constant lane selections or a dynamic mask. Static undefined
lanes and unevaluated index expressions remain explicit. This adds an expression
variant in checked schema 5; it does not change declaration-only schema 3.

`Builtin::X86(X86Intrinsic::Undef128)` identifies Clang’s stable unspecified vector result.
Its signature has no arguments or ISA requirements; the builtin identity does
not imply that its result is a C constant. See [unspecified vector values](undefined-vectors.md).

## Migration from version 4

Checked-code version 5 changes `ExprKind::AlignOf` from a tuple carrying one
`TypeId` to a struct carrying `kind`, `operand`, and `alignment_bytes`.
Declaration-only inspection stays at version 3. In JSON, the previous type query:

```json
{"AlignOf":0}
```

becomes:

```json
{"AlignOf":{"kind":"C11","operand":{"Type":{"occurrence":14,"type_use":0}},"alignment_bytes":4}}
```

`kind` distinguishes C11 `_Alignof` from GNU `__alignof`/`__alignof__`.
An expression operand is `{"Expression": <ExprUse>}` with an unevaluated use
context. Consumers can follow that expression to its source occurrence, lexical
scope, and checked type. The result is the query's alignment in bytes; querying
only the operand's type can lose object and member alignment. See
[expression alignment](expression-alignment.md) for compiler-specific rules.

## Full-width numeric values

Integer values and floating bit patterns serialize as exact decimal JSON numbers,
including values wider than 64 bits. For example, `1u128 << 100` is written as
`1267650600228229401496703205376`, without conversion to a floating-point number or
string. The CLI serializes borrowed typed envelopes directly so `serde_json::Value`
limits cannot cause an inspection panic. Consumers need a JSON decoder that
preserves the integer width they use; decoding these fields directly into Rust
`u128` works without `arbitrary_precision`.

`Builtin::Nontemporal` adds load/store identity with ordinary argument edges.
`Conversion::VectorReinterpret` denotes an equal-size bit reinterpretation,
separate from numeric vector conversion. See [non-temporal accesses](nontemporal-accesses.md).

Both formats also expose `translation_unit.language_mode` (`c11` or `gnu11`).
This additive field records the keywords and preprocessing defaults used to
check the unit. See [language modes](language-modes.md).

## Typedef alignment origins

Declaration schema 3 and checked schema 5 preserve the optional numeric type
`alignment` field. Types can additionally contain an owner-local
`alignment_origin` index into `translation_unit.alignment_origins`. Rows describe
typedef redeclarations and `typeof` layers, including earlier snapshots with no
alignment override. Empty tables are omitted. See [type alignment](type-alignment.md)
for lookup, validation, and common-expression semantics.

## Non-return promises

Declaration schema 3 and checked schema 5 add optional `noreturn: true` fields to
function declarations, entities, declaration sites, and calls. Declaration sites
can also carry `noreturn_source`. Missing fields mean false or no written marker.
The existing `FunctionType.noreturn` field is the separate Clang type contract;
C11 `_Noreturn` does not set it. See [non-returning declarations](noreturn.md) for
snapshot and lexical-scope semantics.

Allocation builtins add `Allocation` operations and GNU `BuiltinFunction` references
to checked expressions. A `BuiltinCall` may also contain an explicit declaration
entity, a captured `noreturn` promise, and a compiler-honored `link_name` override.
These optional fields are additive in checked schema 5; ordinary declaration
symbols continue to use `link_name` in schema 3.
