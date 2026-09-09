# Binding comparison

This validation tool parses Toucan and bindgen output with `syn`, expands typedefs,
and compares function signatures, globals, records, enums, aliases, and constants.
A Rust compiler measures record and enum layouts, ordinary field offsets, enum
discriminants, and constant values. It does not invoke libclang itself.

The crate has its own workspace so the production frontend does not depend on
`syn`. Build it with:

```console
cargo build --locked --manifest-path tools/binding_compare/Cargo.toml
```

Compare two generated Rust files on their native target:

```console
python3 scripts/compare_bindings.py \
  --toucan-bindings /tmp/toucan.rs \
  --bindgen-bindings /tmp/bindgen.rs \
  --target x86_64-unknown-linux-gnu \
  --output /tmp/comparison.json
```

Or regenerate both outputs from the exact commands in a benchmark record:

```console
python3 scripts/compare_bindings.py \
  --benchmark-record benchmark-results/sqlite-reviewed.json \
  --target x86_64-unknown-linux-gnu \
  --output /tmp/sqlite-comparison.json
```

The second form adds bindgen's `--no-prepend-enum-name` and
`--default-macro-constant-type signed` options when the record does not already
specify them. The report records the commands actually used and verifies that the
record's header dependencies remain unchanged during generation. This command
performs no timing; use `scripts/benchmark.py` for measurements.

Pass `--analyzer /path/to/toucan-binding-compare` to use a prebuilt analyzer.
`--skip-native-probes` permits a structural comparison for a foreign target; it
cannot produce an `equivalent: true` result. `--require-equivalent` makes any
remaining mismatch, unsupported construct, missing native validation, or excluded
bitfield storage cause a nonzero exit.

## What is compared

- Foreign functions and globals are grouped by `link_name`, falling back to the
  Rust name. Every public Rust name in a group retains its own type; two names
  sharing a symbol never replace each other. Declaration order is ignored, while
  missing public names and changed linker mappings are differences. Duplicate
  public foreign Rust names produce diagnostics.
- Function signatures preserve the calling convention, pointer constness,
  nullable callbacks, return type, and variadic arguments. Parameter names are
  ignored. A global's outer pointer shape records `static` versus `static mut`
  independently of the declared type's pointer qualifiers.
- Primitive aliases resolve for the seven 64-bit target profiles accepted by the analyzer. `usize` and
  `isize` normalize to their 64-bit representation on those targets.
- Explicit local type re-exports such as `pub use self::First as Later` count as
  public aliases. Groups and alias chains resolve, including private intermediate
  imports; private names do not become public exports. Unresolved public imports,
  value imports, external paths, globs, and cycles remain unsupported diagnostics.
- Records match through shared typedef identities and corresponding positions in
  function and record types. The mapping must be bijective. Field shapes and
  native layouts are compared after that mapping; matching names alone is not
  sufficient. Rust keyword escaping is normalized only when the corresponding
  field uses the original keyword.
- Fieldless enums with one explicit integer `repr` retain their nominal Rust
  names, representations, variant names, and discriminants. Enum types never
  resolve to unrestricted integers. Enum names stay exact; record-name mapping
  does not rename them. Aliases and local re-exports resolve to the same enum.
  Duplicate-value associated constants remain separate `Enum::Constant` exports;
  replacing a variant with a constant remains an API difference.
- Enum discriminants support integer literals, unary negation, parentheses, and
  checked implicit successors. Associated and top-level enum constants resolve
  local variant paths or previously modeled associated aliases. Payload enums,
  generic enums, other representations, conditional/non-exhaustive attributes,
  arbitrary discriminant expressions, and unmodeled public inherent associated items
  produce diagnostics. Probes cast declared variants/constants without creating
  an arbitrary enum value, and check the analyzer's values against compiled Rust.
  Trait implementations and derive-generated APIs are outside this structural
  comparison, as they are for records.
- Bindgen's concrete `__BindgenOpaqueArray<T, N>` aliases retain their tuple
  storage field and native size, alignment, and offset probes. On AArch64 Linux,
  bindgen uses this representation for `va_list`; Toucan exposes its five C
  fields. The corpus gate records that difference and requires matching native
  layouts plus C assertions for every field before accepting it.
- Compiled probes evaluate integer and byte-string constants without substituting
  mathematical values for overflowed or signed values. Constant types are a
  separate comparison; a value match does not hide a type difference.

Every report includes the parsed inventories, native observations, original Rust
names, type/field mappings, exclusions, hashes, and unsupported constructs. A
comparison can expose reference-generator bugs as well as Toucan bugs. Independent
C-compiled probes establish which output agrees with C semantics.

Private generated padding and bitfield storage have different Rust shapes. Their
names are listed explicitly and prevent an overall equivalence claim. Ordinary
fields and complete record size/alignment are still measured. Bitfield getter and
setter semantics require the C/Rust tests in the corpus harness. Opaque records
are compared as opaque types and have no native layout claims.

A passing comparison does not establish ABI register classification, validate
actual foreign calls, or establish behavior for every C input. Run the native FFI
corpus and differential frontend tests as well.

## Analyzer and report schemas

New analyzer inventories and comparison reports use schema version 3. The
`functions` and `globals` maps retain linker-symbol keys; each value is now a list
of exports, sorted by public Rust name, with `rust_name` and `shape` on every
entry, as in schema 2. Comparison results retain their existing category/map format, and compare
all names and shapes within each symbol. Their shared-entry counts count linker
symbols; the inventory lists every public export.

Record correspondence uses matching public names within a shared symbol. A
single export on each side can establish a record correspondence even if its
Rust name changed, but that name difference still fails API equality. Ambiguous
multiple-export groups are never paired by declaration order.

The comparison driver rejects inventories without the current analyzer schema;
rebuild an older `--analyzer` binary before generating new reports. Existing
schema-1 and schema-2 reports remain historical evidence. They cannot recover public aliases
that an older analyzer discarded. Record-only inventory consumers retain the
same `records` schema.

Schema 3 adds an `enums` map keyed by exact nominal name. Each entry stores
`rust_name`, `repr`, and a variant-name-to-decimal-discriminant map. Enum references
use `{"kind": "enum", "value": "Name"}`. Enum-typed constants additionally store
`enum_value`; associated constants use qualified keys and Rust paths. Comparison
reports add `enum_shapes`, `enum_constants`, `native_enums`, and `native_variants`
while preserving existing categories. Native observations add enum size/alignment
and qualified variant values. Integer values are decimal strings, including the
full signed/unsigned 128-bit domain. Rebuild the analyzer and comparison driver
together; schema 2 cannot describe these exports.

## Schema-2 replay evidence

The [saved analyzer and object replay](../../corpus/evidence/binding-symbol-exports-schema2-2026-09-09.json.gz)
records 11 Rust tests, 31 Python tests, and seven CLI controls. It also parses the
unmodified outputs of all 49 multiple-object-name settings, retaining all 51
foreign globals on each side. Every object name and shape matches after the
explicitly Linux-only linker interpretation; two typedef differences and 11
extra helper-record inventories remain visible.

Raw linker spellings remain in the artifact. Bindgen's LLVM no-mangle marker and
Toucan's ordinary spelling identify the same symbol on this Linux ELF target,
whose default symbol prefix is empty. The qualified view does not infer Darwin
or Windows behavior, and does not rewrite analyzer inventories. The original C
FFI evidence is referenced by hash; this replay runs no additional C probes.

## Rustified enum evidence

The [schema-3 capture](../../corpus/evidence/binding-rustified-enums-schema3-2026-09-09.json.gz)
contains source, commands, input/dependency hashes, observations, and comparisons.
The preserved zstd output has six enums and 97 variants; AWS-LC has one enum and
three variants. Both AWS generators and the zstd output parse without unsupported
types. Current Rust and Rust 1.64 execute the extracted, unchanged enum definitions;
GCC and Clang independently read the pinned zstd 1.5.7 and AWS-LC 0.44.0 headers.
All seven layouts and 100 discriminants agree on x86-64 Linux.

Signed, unsigned, nonzero-only, duplicate-value, alias, and nominal-type controls
cover the added model. A native CLI positive passes; changes to discriminants,
representations, names, alias presence/values, parameter enum identity, and integer
field substitution fail. Unsupported expressions and a schema-2 analyzer fail
explicitly. The capture includes 16 Rust tests, 34 Python tests, and Clippy.

Current Rust also validates i128/u128 discriminants. Rust 1.64 rejects those enum
representations with E0658; that rejection is retained. These hand-written analyzer
controls agree with Toucan's existing Rust 1.89 minimum for 128-bit rustified enum
output. The MSRV result covers the
consumer u32 enums and the i32/u32 controls. These probes make no new consumer,
FFI-call, ABI-register-classification, cross-target, or performance claim.

## Tests

```console
cargo test --locked --manifest-path tools/binding_compare/Cargo.toml
cargo clippy --locked --manifest-path tools/binding_compare/Cargo.toml --all-targets -- -D warnings
python3 -m unittest discover -s tools/binding_compare/tests
```
