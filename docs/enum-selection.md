# Selecting Rust enums

`BindingOptions::rustified_enum_patterns` selects individual C enums by exact name
or a prefix ending in `*`. The Builder spelling uses `.*`, with optional `^` and
`$` anchors. For example:

```rust,ignore
let bindings = toucan_bindgen::Builder::default()
    .header("rust_wrapper.h")
    .rustified_enum("point_conversion_form_t")
    .generate()?;
```

This is the selection used by AWS-LC's binding build script. Its anonymous enum
defines values 2, 4, and 6. Those named variants become Rust enum values; other C
enums keep their integer representation. Values outside the selected enum's
declared variants remain invalid Rust values. The caller must ensure C functions
and memory accesses respect that restriction.

## Name matching

- A named enum matches its C tag.
- An anonymous enum matches its first typedef name. Later aliases do not match.
- An anonymous enum without a typedef matches its enumerator names. Selecting one
  enumerator emits the complete Rust enum, including the other valid variants.
- A style pattern does not select declarations excluded by the name allowlist.
- Enum bitfields retain the existing requirement to use integer enum storage;
  selecting an unrelated enum does not prevent generating those bitfields.

The existing `rustified_enums: true` option, and Builder `rustified_enum(".*")`,
select all enums. Default options continue to emit integer aliases and constants.

Builder selectors use the scoped enum names described in
[enum constant names](enum-constant-names.md). For example, an enum `Nested`
inside `struct Holder` can match `Holder_Nested`. The semantic owner metadata and
bounded declaration discovery preserve these names independently of uses of the
enum type. The core library's default enum style continues to match C tags or
first typedef names; selecting its bindgen enum style enables the scoped names.
Arbitrary regex syntax remains unsupported for Rust enum style selectors.

## Validation

The [reference capture](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/selective-enums-2026-09-08/README.md)
contains thirteen bindgen 0.72.1 selection cases, complete commands and output,
and the original nested-name difference. The later
[lexical naming checks](enum-constant-names.md) cover scoped selectors. Tests
cover canonical tags, first typedefs,
later aliases, anonymous constants, selection roots, and enum bitfields across
the supported targets. A native C/Rust test roundtrips all three AWS-LC
point-conversion values alongside an integer enum through GCC and Clang objects.
The generated source also compiles and executes with Rust 1.64.

This implements the enum option required by AWS-LC. It is not evidence that its
complete binding build script or consumer passes; those require the other
requested options and a separate unchanged-source consumer run.
