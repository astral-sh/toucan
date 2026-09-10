# Documentation in generated bindings

`toucan_bindgen::Builder::generate_comments(true)` is the default. C documentation
comments become Rust `#[doc = "..."]` attributes on selected functions, objects,
typedefs, records, enumerations, named fields, and enum constants or variants.
`generate_comments(false)` disables physical comment capture, declaration-location
retention, attachment, and emission.

The adapter accepts `-fparse-all-comments` to include ordinary comments and
`-fretain-comments-from-system-headers` to include comments written in system
regions. These are parsed as flags, preserving include and macro operands that
happen to use the same spelling. System status belongs to the comment's physical
start, independently of a later `#line` or system-header transition.

## Attachment and output

Attachment follows the pinned bindgen 0.72.1/libclang 18 behavior. Typedefs search
from the declaration's first token; other named declarations search from their
identifier. Trailing field, enumerator, and object comments must share a physical
line. Leading comments stop at intervening declaration barriers. Blank lines do
not themselves prevent attachment. The first attached comment wins across
redeclarations, including an empty comment that suppresses later documentation.

A forward tag introduced by a file-scope typedef, object, or function declaration
does not acquire that declaration's comment. An actual standalone forward tag can
carry its own comment, and an inline tag definition can share a typedef's comment.
This keeps AWS-LC's `EVP_ENCODE_CTX` typedef documentation off the later
`evp_encode_ctx_st` definition. The
[focused forward-tag comparison](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/forward-tag-documentation-2026-09-09.json.gz)
covers later definitions, opaque records, same-name aliases, and redeclarations
across physical source files.

Macro declarations first search the invocation, then the declaration-begin
spelling. A member whose containing record or enumeration is also macro-expanded
uses its spelling location. This prevents a comment on a generated container
from being copied onto its children. A configured `-D` replacement has no physical
spelling location. Declaration locations and physical provenance are retained
separately from diagnostic line remapping.

Comment text uses bindgen's physical-line normalization: C comment markers and
line-leading stars are removed while Doxygen commands, trailing `<` markers, raw
line splices, quotes, backslashes, and Unicode remain text. Rust string escaping
preserves that text in the generated attribute. Raw lines supplied by the caller
remain outside this processing and outside rustfmt, as before.

Documentation follows the selected C item through generated name changes and
foreign-block placement. Synthetic anonymous storage fields receive no duplicate
comment; their underlying anonymous record and named members can retain comments.
Bitfield storage and associated constants for repeated Rust enum values receive
no documentation, matching the pinned reference. Macro constants, function
parameters, and a `ParseCallbacks::process_comment` API are outside this layer.

## Reusable frontend APIs

`AnalysisOptions::retain_documentation_origins` retains immutable
`DocumentationDeclarations` independently of checked bodies. Each occurrence
identifies its C declaration, tag, field, or enumerator and the begin/name tokens
and immediate containing tag's name token. These coordinates do not change C type
identity or lexical scope. The facade enables this catalog only when preprocessing
captured comments; standalone semantic analysis honors the option directly.

The core emitter accepts `BindingOptions::documentation`, containing text keyed
by original C declarations and canonical record/field identities. It validates
those keys before emission. Core and CLI defaults leave this option disabled.
The attachment catalog is limited to one million occurrences; emitted text is
limited to one million items and 64 MiB, charged before cloning a reused macro
comment. The preprocessing catalog retains its separate provenance limits.

## Validation

The maintained [adapter tests](../crates/toucan_bindgen/tests/documentation.rs)
cover attachment, macro spelling, renamed scalar and string constants, anonymous
storage, repeated enum values, and Rust compilation of escaped documentation.
[Semantic origin tests](../crates/toucan_semantic/tests/documentation_origins.rs)
and [preprocessing tests](../crates/toucan_preprocessor/tests/documentation.rs)
cover the underlying declaration and physical comment catalogs.

The [historical comparison](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/documentation-emission-2026-09-09.json.gz)
and [composition checks](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/documentation-composition-2026-09-09.json.gz)
retain pinned bindgen item/text comparisons, Rust-version checks, and allocation
measurements. They cover the recorded source revisions, including documentation
on full-width integer constants and file-selected redeclarations. Their header
and allocation measurements do not establish current performance or a consumer
runtime result.
