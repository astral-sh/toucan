# Expression alignment

The frontend checks `_Alignof`, `__alignof`, and `__alignof__` with type-name or
expression operands. Type-name operands retain their written type use. Expression
operands are checked without executing them: calls, increments, statement
expressions, and newly written VLA bounds remain unevaluated. A VLA typedef created
before the query retains its original bound evaluation and identity.

`checked::ExprKind::AlignOf` exposes the spelling kind, owned operand, and result
in bytes. Expression operands link to an unevaluated `ExprUse`; source occurrences
and referenced entities remain available through the ordinary checked-code API.
[Inspection version 5](inspection.md#migration-from-version-4) describes the JSON
migration. `TranslationUnit::record_field_alignment` reports a non-bitfield's
alignment under the immediate containing record's packing and field attributes,
without changing that record's layout.

## Object and type alignment

An aligned object's query can differ from a query of its type:

```c
int value __attribute__((aligned(32)));
_Static_assert(__alignof__(value) == 32, "object");
_Static_assert(__alignof__(int) == 4, "type");
```

A direct member query uses the member's field alignment, including packing and
explicit field alignment. Overall object or record alignment does not raise every
member's alignment. Nested members use their immediate record's rules. Queries of
bitfields and sizeless SVE values produce diagnostics. GNU incomplete object
expressions have alignment 1 unless explicitly aligned; Clang requires a complete
object even when its declaration has an explicit alignment.

GNU and Clang differ in pointer-expression propagation. GNU preserves an object's
alignment through its address, equivalent pointer casts, and integer-constant zero
offsets. A cast to a different pointee uses the greater natural alignment of the
original and final types; casting back can restore the declared alignment. Clang
dereferences use the final pointee type's alignment. Array decay, nonzero or
nonconstant offsets, conditional pointers, and comma expressions do not carry an
object's explicit alignment. These are compiler query rules, not a general proof
that a runtime pointer is aligned. GCC casts also remove top-level typedef
alignment, while Clang casts retain it; pointee typedef alignment is separate.

Unannotated void queries return 1. Without an explicit declaration alignment, function queries
return 1 for GNU x86-64 Linux (GNU or musl libc) and 4 for the other supported profiles.
Clang preserves GNU-spelled alignment on void/function typedefs; GCC ignores it.
An explicit function declaration alignment is separate from a typedef's type
alignment. GCC keeps AArch64 functions at a minimum of 4 bytes; Clang reports the
written declaration alignment even below that value. Clang aligned
parameter queries use the parameter's storage annotation inside its definition;
the parameter's incoming C type remains unchanged.

## Bounds and validation

Origin tracking is active only while checking an expression alignment operand.
`ExpressionInfo` remains 64 bytes on the measured x86-64 host. The analyzer caps
both query results and origin entries at 65,536, and caps cloned origin type
payloads at 16 MiB before allocation. Type and anonymous-member walks retain the
128-level depth limit. Retained origin links also debit the configured payload
budget. Code without these queries does not allocate origin entries or links.

The [evidence report](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/expression-alignment-2026-09-08.json)
records compiler comparisons, native side-effect checks, generated Rust 1.64
constants, and allocation-byte/RSS measurements on real headers and source files.
Native execution is limited to the available x86-64 Linux host; cross-target
compiler checks do not establish runtime ABI equivalence.

Compiler references: [GCC alignment queries](https://gcc.gnu.org/onlinedocs/gcc/Alignment.html)
and [Clang language extensions](https://clang.llvm.org/docs/LanguageExtensions.html#alignof-alignof).

## Resource measurements

The before/after corpus includes four real header interfaces and seven
compiler-preprocessed source files, in ordinary and retained modes, with three
randomized paired samples. All 132 runs accept their inputs, and every generated
header output is byte-for-byte unchanged. Ordinary analysis requests 0–376
additional bytes per input. Retained source-file analysis requests 48 or 424
additional bytes. Retained header samples vary by one allocation request; the
report preserves each measurement rather than attributing that variation to the
feature. These are cumulative allocation requests, not live memory sizes.

Across 56 small cases without alignment queries, allocation counts and generated
bindings are unchanged. Ordinary requested bytes are identical; retained analysis
adds 48 bytes to the existing builder allocation. The origin map remains empty.
The real-input report includes process peak RSS and timings, but three
allocator-instrumented samples do not establish an end-user speed difference.
