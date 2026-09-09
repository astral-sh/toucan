# Binding comparison

This validation tool parses Toucan and bindgen output with `syn`, expands typedefs,
and compares function signatures, globals, record fields, aliases, and constants.
A Rust compiler measures both outputs' record sizes, alignments, ordinary field
offsets, and constant values. It does not invoke libclang itself.

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

- Foreign functions are identified by `link_name`, falling back to the Rust name.
  Signatures preserve the calling convention, pointer constness, nullable callback
  representation, return type, and variadic arguments. Parameter names are ignored.
- Primitive aliases resolve for the five targets supported by Toucan. `usize` and
  `isize` normalize to their 64-bit representation on those targets.
- Records match through shared typedef identities and corresponding positions in
  function and record types. The mapping must be bijective. Field shapes and
  native layouts are compared after that mapping; matching names alone is not
  sufficient. Rust keyword escaping is normalized only when the corresponding
  field uses the original keyword.
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

## Tests

```console
cargo test --locked --manifest-path tools/binding_compare/Cargo.toml
cargo clippy --locked --manifest-path tools/binding_compare/Cargo.toml --all-targets -- -D warnings
python3 -m unittest discover -s tools/binding_compare/tests
```
