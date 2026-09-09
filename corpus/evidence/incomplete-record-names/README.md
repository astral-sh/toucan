# Incomplete record names

Base frontend: `7db2b85027108be1515ad76263720e665cba2ab9`.

A record member can introduce a file-scope incomplete C tag:

```c
typedef struct { struct T *data; } Owner;
```

Pinned bindgen 0.72.1 exposes `T`; Toucan previously exposed `Owner_T`.
Code naming `T` failed to compile with Toucan even though a structural comparator
could pair the records and consider the pointer signatures equivalent. The
correction changes the Builder naming policy for named incomplete file-scope
records. Completed nested definitions retain their lexical names; C type
identities, opaque storage, and core default naming are unchanged.

Two boundaries remain deliberate. With an unrelated `typedef int T`, Toucan
keeps `Owner_T` so both Rust types remain usable; bindgen emits duplicate `T`
types and fails Rust compilation. A `T` first declared in a callback prototype
remains distinct from a later file-scope `T`. GCC 13 and Clang 18 reject the
incompatible callback assignment; bindgen merges these types in the control.

Validation on native x86-64 Linux:

- All 19 C fixtures are accepted by GCC 13 and Clang 18. Public tag-name sets
  match in 18 cases; the prototype-scope distinction explains the remaining case.
- All 38 generated-fixture compilations pass on Rust 1.64 and current Rust.
  Sixteen additional named-use, selection, collision and scope checks have the
  expected acceptance or rejection. Both collision outputs are unchanged.
- Ten C scope checks confirm compatible field/file declarations, later completion,
  rejection of incomplete `sizeof`, and rejection of mixed prototype/file scope.
- Four new Rust tests, five existing lexical/prototype tests and focused Clippy pass.

The AWS-LC all-bindings generation replay uses its untouched pinned
`builder/sys_bindgen.rs`. The baseline reproduces the archived output exactly.
The candidate changes only eight occurrences of
`CRYPTO_dynlock_CRYPTO_dynlock_value` to `CRYPTO_dynlock_value`; a direct named
pointer use then compiles. No consumer build or runtime check was repeated.
The private opaque representation stays unchanged; bindgen's public `_address`
byte is not a C layout oracle for an incomplete tag.

`capture.json.gz` retains sources, generated bindings, exact command results,
compiler diagnostics, source and artifact hashes, and cache-owned reproduction
runners. The first AWS replay's extra prelude and its corrected upstream option
are both preserved. Large compiled artifacts stay at recorded cache paths.
This evidence does not claim another target, complete bindgen API equality, or
by-value support for incomplete records. Comparator normalization is separate.
