# Empty scalar initializers

GCC 13.3 and Clang 18.1.3 accept `int x = {};` as a non-pedantic extension in
C90, GNU90, C11, and GNU11. Toucan supports this extension for complete scalar
objects: integers, enums, floating and complex types, and pointers. Nested
scalar braces, scalar subobjects, automatic objects, and scalar compound
literals also work. The initializer produces the destination type's zero value,
including positive floating zero and null pointers.
The extension is described in the [GCC 13 release notes](https://gcc.gnu.org/gcc-13/changes.html)
and [Clang's implementation review](https://reviews.llvm.org/D147349).

Retained `InitializerKind::List` records an empty entry list with
`zero_fill_unwritten = true`. For a scalar, the flag initializes that entire
object. Its type and written occurrence remain available; no synthetic integer
expression or per-byte storage is allocated. The flag does not promise zeroed
padding bytes. Arithmetic constant queries return zero in the destination's
integer or floating format, including both complex components.

Clang rejects braced `_Atomic` initializers, including `{}`, `{0}`, and nested
braces. GCC accepts them. This distinction applies to scalar and aggregate atomic
destinations; expression initializers keep their existing behavior.

Existing aggregate rules remain in place. Empty variable-length arrays and
Clang's nested vector-lane braces are separate unsupported extensions. Scalar
compound-literal folding in integer-constant-expression contexts remains a
separate compiler difference. Excess scalar entries still produce a diagnostic,
including cases where the native compiler warns and ignores the extra entries.

The [evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/empty-scalars-2026-09-08/summary.json) records
480 native admission observations across four modes and compiler targets,
including the unsupported boundaries above. Sixteen native executable probes
check zero values, null pointers and positive floating/complex zero with GCC and
Clang at O0/O2 under UndefinedBehaviorSanitizer. Ordinary and retained tests
cover the supported profiles, actual source spans, atomic distinctions, and
bounded nesting. These are conformance observations, not a performance claim.

The [combined C90 integration](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/empty-scalars-root-integration-2026-09-08/summary.json)
reruns the complete 184-case CLI table against GCC 13.3 and Clang 18.1.3. All
184 decisions agree, closing the four previously recorded scalar-empty gaps.
It also checks 3,784 ordinary/retained pairs across 44 profile/mode settings and
preserves the eight real-header and musl ABI binding artifacts.
