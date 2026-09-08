# Vector lane conversions

`__builtin_convertvector(value, type)` converts corresponding lanes numerically.
The source and destination must be fixed vector types with equal lane counts.
Their element widths and total sizes can differ. For example, converting four
32-bit integers to four 16-bit integers changes the vector size from 16 to 8
bytes. Floating-to-integer conversions truncate toward zero and retain the
ordinary scalar conversion's representability requirements.

The input is evaluated once, including volatile reads and GCC atomic reads. The result is
an rvalue. Written destination qualifiers and alignment are retained; enclosing
value conversions apply as usual. GCC accepts atomic-qualified input and destination types,
while Clang rejects those spellings. The ordinary vector policy still limits
storage to 16 bytes and power-of-two vector sizes. Sizeless SVE types are not
fixed vectors.

With retained analysis, `ExprKind::ConvertVector` owns one value `ExprUse` and a
`TypeNameOperand`. The latter links the written destination to its checked type
and source occurrence. This node identifies numeric lane conversion separately
from a vector bit reinterpretation. Queries and discarded branches preserve
their existing evaluation policies. The node does not establish a Rust vector
calling convention or machine-code lowering.

The implementation follows the supported GCC 13 and Clang 18 profiles. The
[GNU vector extension documentation](https://gcc.gnu.org/onlinedocs/gcc/Vector-Extensions.html)
and [Clang language extension documentation](https://clang.llvm.org/docs/LanguageExtensions.html#builtin-convertvector)
describe the operation. Tested vector literals do not make this builtin a C
integer constant expression or a supported static initializer; this layer adds
no vector constant evaluator. Native compiler and runtime results, along with
ordinary allocation comparisons, are recorded in
[the validation evidence](../corpus/evidence/convert-vector-2026-09-08.json).
