# Reading earlier const objects in initializers

The Clang profile can use a completed file-scope const scalar definition in a later static initializer. For example, `static const int first=7; static const int value=first+1;` produces the checked value `8`. Integer, Boolean, enum, and floating values retain their destination conversions. Const atomic scalars and thread-local scalars follow the same Clang rule. An enum object can supply an arithmetic value while its own binding remains an extern declaration.

Values become available only after the initialized definition passes ordinary checking and its optional binding value has been captured. A self-query initializer such as `static const int value=__builtin_constant_p(value);` therefore remains `0`. Forward references, tentative definitions, mutable or volatile objects, and definitions already declared weak cannot supply values. Local objects and parameters hide the file spelling; a compatible block `extern` can refer to the file definition. Redeclarations do not replace the saved definition value. The existing rejection of a weak attribute added after a definition remains a separate admission gap, although Clang accepts that placement.

Clang also knows these completed values in `__builtin_constant_p`, both at file scope and inside a function. This keeps the builtin consistent with earlier cached `__builtin_choose_expr` selections. The object name is still absent from the ordinary integer-constant-expression table. Direct const-object reads in enum values, static assertions, and array bounds retain their existing diagnostics. A query such as `__builtin_constant_p(first)` can itself return the integer constant `1`.

The GNU profile is unchanged. The new table exists only while analyzing a Clang translation unit, is allocated lazily, and retains scalar values without expression or body graphs. It is bounded to 100,000 objects and 64 MiB of accounted name/value/map storage. Unsupported seed evaluation leaves an already accepted declaration accepted. It cannot supply a later value. Independent queries on a completed `TranslationUnit` do not reconstruct this table.

Pointer copies, aggregate elements, and local const definitions remain separate work. The table does not add new arithmetic builtins or extend the existing evaluator's operand coverage. Builder's first-selected-occurrence, physical-file filters, name callbacks, unsigned 64-bit fallback, and core default emission policy are unchanged.

## Validation

The [semantic tests](../crates/toucan_semantic/tests/const_object_reads.rs) cover
source order, conversions, shadowing, query caching, retention, and resource
limits. [Builder object tests](../crates/toucan_bindgen/tests/object_values.rs)
check emitted types and values. Optimizer-dependent knowledge,
such as a mutable local becoming constant after optimization, remains outside the
frontend's constant proof.

[Historical initializer observations](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/const-object-initializers-2026-09-09.json.gz)
retain the reference differences, native values, and original commands.
