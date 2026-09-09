# Function typedef bindings

The Builder represents a bare C function typedef as a nullable Rust callback,
matching bindgen. For example:

```c
typedef int Callback(int);
typedef Callback Alias;
extern Callback *current;
extern Callback **slot;
Callback declared;
```

The generated aliases and objects have these Rust types:

```rust
pub type Callback = Option<unsafe extern "C" fn(::core::ffi::c_int) -> ::core::ffi::c_int>;
pub type Alias = Callback;
extern "C" {
    pub static mut current: Callback;
    pub static mut slot: *mut Callback;
    pub fn declared(value: ::core::ffi::c_int) -> ::core::ffi::c_int;
}
```

This lets an existing Rust consumer write `let callback: Callback = None`.
Function-pointer aliases, parameters, return types, nested callbacks, arrays and
record fields reuse the alias. A pointer to the function has one nullable layer;
a pointer to that pointer remains a raw pointer to the nullable callback.

`BindingOptions::nullable_function_typedefs` enables this binding policy. The
Builder sets it to true; core and CLI defaults keep their existing non-null
function aliases. Enum formatting does not change this policy. The semantic
translation unit still distinguishes C function types from pointer types, and
bare function declarations still generate Rust extern functions. Existing
calling-convention and ABI checks remain active.

Blocked callback aliases retain their names and referenced type dependencies.
Their caller-owned Rust definitions must use the same nullable function-pointer
representation, signature and calling convention. Their C type remains a
function, so the external-type report does not invent a C object layout for it.

## Evidence

The [saved Linux x86-64 comparison](../corpus/evidence/function-typedef-bindings-2026-09-09.json.gz)
uses bindgen 0.72.1/libclang 18.1.3. Eleven of twelve small-header comparisons
match the complete public API exactly, including callback nullability and object
mutability. All 48 generated Rust compilations pass on current Rust and Rust 1.64.
The remaining attributed-function case preserves a preceding difference:
bindgen omits `sysv_abi`/`ms_abi` typedef exports, while Toucan emits them. The
globals' callback shapes and calling conventions match. That difference is
recorded without removing the additional aliases.

The study also generates the three real `CRYPTO_EX_free`, `CRYPTO_EX_dup` and
`EVP_PKEY_gen_cb` aliases directly from the pinned aws-lc-sys 0.44.0 headers.
Their callback API shapes now match, and four Rust compilations accept typed
`None` values. The full analyzer records retain the existing private opaque
placeholder spelling difference (`_unused` versus `_private`).

Eight C/Rust executions cover GCC/Clang, both generators, and both Rust versions.
They pass null and non-null callbacks in both directions, return callbacks,
call nested callback factories, access double pointers and global arrays, and
pass and return a struct containing callbacks. Three focused regressions also
check the unchanged core policy, C function identity, blocked dependencies and
Rust source compatibility. No full application consumer rebuild is claimed.

## Caller-built type graphs

The nullable projection validates selected alias dependencies before emission.
Self-referential and mutually recursive aliases produce a diagnostic; named
records and enums end alias expansion, preserving legal record/callback cycles.
The borrowed traversal memoizes completed aliases, limits depth to 256 and total
work to one million type nodes, and does not clone or mutate semantic types.
The core default returns before allocating validation state.

The [focused graph checks](../corpus/evidence/function-typedef-cycles-2026-09-09.json.gz)
cover both projection policies, mutual aliases, and shared record/callback types.
They supplement the native API capture above.

## Recursive work limit

Binding generation shares a four-million-entry allowance across recursive type
rendering, signature rendering, function ABI checks, and by-value record checks.
Each generation starts with a fresh private counter, under both callback
projection policies. The existing depth diagnostic takes precedence. This
bounds repeated traversal of shared type graphs; it is not a global output-byte
limit or a limit on all frontend work.

The [work-budget capture](../corpus/evidence/binding-work-budget-2026-09-09.json.gz)
checks unchanged output for core and Builder routes on zlib, SQLite, zstd, and
libgit2. All eight comparisons match their preceding output hashes. The largest
case uses 11,368 entries, or 0.2842% of the allowance. A separate instrumented
binary records counts; production code contains only the private counter.
Small injected-budget tests check both rejection and unchanged output.
