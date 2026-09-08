# Inspect checked C code

Use `inspect --checked-code` to export the information available to analyzers and
header indexes:

```console
toucan inspect api.h --target x86_64-unknown-linux-gnu --checked-code --output api.json
```

The command preprocesses and checks the entire input, including function bodies.
Its version 2 JSON contains:

- `translation_unit`: target and compiler identities, declarations, canonical types,
  records, and enumerations.
- `checked_code`: owned arenas for source occurrences, scopes, entities,
  declaration sites, expressions, bodies, initializers, and type uses.
- `preprocessed`: the exact source addressed by those occurrences and mappings to
  original file locations or macro invocations.

Without `--checked-code`, `inspect` keeps the version 1 declaration-only format.
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
