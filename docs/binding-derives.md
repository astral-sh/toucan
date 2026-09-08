# Traits for generated bindings

The Builder supports `derive_copy`, `derive_debug`, `derive_default`, `derive_eq`,
and `derive_partialeq`. The library exposes the same requests through
`BindingOptions::derives` and `DeriveOptions`. Requests apply to generated storage
types when their fields support the trait; integer aliases and raw pointers keep
the traits supplied by Rust.

AWS-LC requests the following combination:

```rust,ignore
let bindings = toucan_bindgen::Builder::default()
    .header("rust_wrapper.h")
    .rustified_enum("point_conversion_form_t")
    .derive_copy(true)
    .derive_debug(true)
    .derive_default(true)
    .derive_eq(true)
    .generate()?;
```

Default options preserve the existing output: copyable records receive `Clone`
and `Copy`, and Rust enums also receive `Debug`, `PartialEq`, `Eq`, and `Hash`.
`DeriveOptions::debug: None` retains that per-kind behavior; the Builder's
`derive_debug` method supplies an explicit override. Enabling Eq also requests
PartialEq. Disabling Eq leaves an earlier PartialEq request intact; disabling
PartialEq disables both requests. Rust enums retain Clone and their equality and
hash traits independently, matching bindgen's compatibility policy.

## Storage and validity

Default initializes zero storage. It does not call a C library constructor.
Generation proves that zero is a valid Rust representation before emitting a
Default implementation. This supports pointers, optional callbacks, arrays
longer than 32 elements, ordinary records, and unions without requiring every
field type to implement Default itself.

A rustified enum receives no Default implementation. A struct containing one can
receive Default only if the enum declares zero; empty arrays contain no enum
value. For example, AWS-LC's point-conversion enum declares 2, 4, and 6. A struct
containing that Rust enum receives no Default. Bindgen 0.72.1 emits a zeroing
Default for such a struct; reproducing that output would construct an invalid
Rust value. Toucan omits the trait and still generates compilable bindings.

A union has no active member invariant. Default can zero its storage even when
one member's type cannot contain zero. Reading a union member remains unsafe and
requires a valid representation for that member. Atomic storage, caller-owned
external storage, and incomplete C records do not receive generated defaults.
Typedef aliases preserve the caller-owned type boundary: an alias of an external
type cannot inherit the underlying C type's traits or zero validity. Pointers to
those aliases still have the traits and null representation of Rust pointers.

The [combined-layer checks](../corpus/evidence/binding-derives-root-integration-2026-09-08.json.gz)
include the external-alias correction found during independent review. Eight
focused tests pass with current Rust and actual Rust 1.64 consumers. The frozen
reference and sanitizer captures predate that correction; they do not establish
the corrected alias behavior.

Disabling Copy also removes Clone from generated records. Union members whose
generated type becomes non-Copy use `ManuallyDrop`, preserving their layout.
Generated unions do not implement Debug or equality. These restrictions propagate
through containing records. Packed records without Copy conservatively omit
Debug and equality, as bindgen does.

Float fields support PartialEq, but not Eq. Pointer and supported callback
equality compares addresses. Callback Debug/equality follows the emitted ABI:
an explicit convention equal to the target default uses Rust's C convention.
Other conventions and signatures with more than twelve parameters retain
bindgen's conservative derive boundary. Explicit `MaybeUninit` bitfield padding
prevents equality derivation; it is never read to implement comparison. Zero-length
and flexible array fields use Rust arrays, which support Copy and equality;
bindgen represents these fields with a helper that omits those traits.

## Validation and limits

Trait eligibility memoizes shared record graphs and does not follow pointer
cycles into another object's storage. The walk has the existing 256-level type
depth limit and a one-million-visit limit. Atomic and external containment queries
use their prepared record caches. Default configuration needs no new trait cache
and keeps the existing borrowed derive attributes.

The [evidence](../corpus/evidence/binding-derives-2026-09-08/README.md) records
bindgen 0.72.1 trait requests, C/Rust Default checks, Rust 1.64 checks, and the
actual AWS-LC header type inventory. That inventory selects type declarations
from the wrapper's thirty allowed headers; it does not implement the separate
function, callback, and file-selection policies of the complete build script.
The full unchanged-source AWS-LC consumer remains an integration gate.
