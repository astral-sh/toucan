# Array typedef qualifiers

C11 applies qualifiers written on an array typedef to its element type. These
declarations therefore describe the same object:

```c
typedef const int First[4];
typedef int Second[4];
extern First shared;
const Second shared = {1, 2};
```

Type compatibility and typedef identity compare the combined qualification of
consecutive array layers. Pointer pointees start a new qualification boundary.
The comparison retains array extents, VLA identity rules, stored typedef names
and existing source types. It does not change layout, composite-type construction,
parameter adjustment, qualification conversions or profile-specific `restrict`
admission.

The [saved C11 controls](../corpus/evidence/array-typedef-qualifiers-2026-09-09.json.gz)
record 31 cases checked with GCC and Clang: 62 native admission results and 124
Toucan checks with code retention enabled and disabled. They cover definition
order, complete/incomplete bounds, `const`/`volatile`/`restrict`, multidimensional
arrays, pointer and parameter boundaries, VLA typedefs, `_Generic` and type
compatibility queries. Incompatible qualification and bounds remain rejected.
GCC-only qualification of pointer-array typedefs remains profile-specific.

The three focused tests pass, including the native compiler check. Another 61
existing array, expression, introspection, deduction and declaration tests pass;
their nine optional native tests were not rerun. Clippy and formatting pass.
The artifact includes sources, compiler versions, commands, hashes and logs.
This evidence establishes syntax admission and retained type invariants; it
contains no new runtime, ABI or performance measurements.
