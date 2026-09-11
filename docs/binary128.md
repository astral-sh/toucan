# Binary128 types and constants

Toucan preserves the IEEE binary128 interchange type as
`FloatKind::FLOAT128`, an alias for the existing
`Extended { format: BinaryInterchange, width: 128 }` variant. Its size and
alignment are 16 bytes. A complex value has two such components and occupies
32 bytes with 16-byte alignment. Atomic scalar and complex layout follows the
selected target profile; this does not imply lock-free access.

Type spelling, literal suffixes, and machine modes have different availability:

| Route | GNU x86 Linux | GNU ARM Linux | Clang x86 Linux | Other Clang profiles |
| --- | --- | --- | --- | --- |
| `__float128` | Shadowable predefined typedef | Ordinary identifier, no builtin type | Reserved type spelling | Reserved spelling, unsupported type |
| `_Float128` | Distinct binary128 type | Distinct binary128 type | Ordinary identifier, no builtin type | Ordinary identifier, no builtin type |
| `Q` / `q` literal | Binary128 | `long double` | Binary128 | Binary128 |
| `f128` / `F128` literal | Binary128 | Binary128 | Unavailable | Unavailable |
| `mode(TF)` / `mode(TC)` | Binary128 real / complex | `long double` real / complex | Binary128 real / complex | ARM Linux: `long double`; Darwin and Windows: unavailable |

Imaginary literal suffixes retain the same component identity. On GNU ARM,
`_Float128` and `long double` have equal formats but remain distinct C types;
`_Float128` wins their usual arithmetic conversion. A predefined GNU typedef
can be shadowed by a local identifier or replaced by an explicit typedef
without changing types that were already checked. Toucan does not emit that
implicit typedef as a public declaration. Evaluation fragments use the same
compiler-specific name rules as translation units.

GNU x86-64 rejects an object or function with linkage named `__float128`, including
block-scope `extern` declarations. Local objects, parameters, tags, and enumerators
can use the name; explicit typedefs can replace the predefined alias. GNU AArch64
has no predefined `__float128` type and permits ordinary file-scope names. Clang
reserves `__float128` even on targets where that floating type is unavailable.

`mode(SF/DF/SC/DC)` selects the corresponding standard real or complex type.
Floating modes validate the completed declaration subject, including parameters;
a pointer, array, or function declaration cannot acquire a scalar machine mode.
Clang ignores mode attributes inside a type name, matching its declaration-only
attribute behavior. Its real mode on a complex declaration produces a real type;
GNU C rejects that combination.

Scalar arithmetic and conversions use target-format APFloat encodings. Checked
expressions keep real and complex operand domains separate and preserve atomic
read-modify-write operations. Existing complex constant-folding limits still
apply: GNU complex multiplication or division whose final rounding cannot be
proved reports an explicit unsupported constant diagnostic. This is not a claim
of Annex G or floating-environment support.

## Rust boundary

Binary128 storage and call ABIs have not been proved against Rust. Selected
interfaces therefore report an explicit binding diagnostic, including aliases,
callbacks, records, and atomic storage. Binary128 and complex macro constants
are reported as skipped; an explicit C `float` or `double` conversion can produce
a supported Rust scalar constant. No type is emitted as a Rust struct merely
because its size and alignment agree.

## Evidence

The focused tests compare ordinary and retained analysis across seven profiles,
compiler acceptance, type identity, layout, atomic layout, and native O0/O2
constant bytes. Cross-target assembly probes include GNU ARM and all Clang
profiles. Untouched FFTW, LAPACKE, and glibc headers are checked through both
Toucan preprocessing and compiler-preprocessed input, preserving line markers.
The accompanying evidence records commands, source and compiler hashes, versions,
and any remaining diagnostics; partial failures are excluded from timing claims.

The implementation follows the [GCC floating-type documentation](https://gcc.gnu.org/onlinedocs/gcc/Floating-Types.html),
[GCC 13.3 predefined type registration](https://github.com/gcc-mirror/gcc/blob/releases/gcc-13.3.0/gcc/c-family/c-common.cc),
and [Clang 18.1.3 mode attribute handling](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/clang/lib/Sema/SemaDeclAttr.cpp).
The native observations are compiler-version evidence; they do not establish a
Rust ABI or support for every compiler release.

Reproduce the focused checks with:

```console
cargo test -p toucan_semantic --test float128 -- --include-ignored
cargo test -p toucan --test float128 -- --include-ignored
```

The native checks require the named compilers; missing tools and compiler crashes
are failures. GNU native runtime checks cover the shipped Linux profiles, and
Clang checks all five cross targets plus the native target. Set `TOUCAN_GCC` to
select GNU GCC and `TOUCAN_TEST_RUST_TOOLCHAIN=1.64.0` for the generated Rust check.

Earlier header, allocation, and timing observations remain in the
[historical archive](validation.md#historical-results).
