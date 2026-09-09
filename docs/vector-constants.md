# Fixed-vector constant evaluation

`evaluate_vector(unit, expression)` folds supported fixed-vector expressions and
returns an owner-independent value/storage shape and target-encoded lane values.
`VectorConstant::ty()` preserves resolved scalar and vector kinds, qualifiers, and
effective alignment bytes. Complete typedef identity and ancestry stay with the
original analysis; temporary evaluator IDs never escape this snapshot. Lanes retain
integer widths and floating formats, including signed zero and subnormals.
The query uses the unit's compiler, target, and language mode.

A folded value does not certify a C integer constant expression or a valid
static initializer. Clang 18 can fold some vector operations for static storage
while rejecting a literal vector's numeric `__builtin_convertvector` initializer.
GCC 13 admits identity conversions, but rejects numeric conversions and most
vector arithmetic at those sites. Toucan checks that distinction separately.
The existing conservative `__builtin_constant_p` policy is unchanged.

## Supported expressions

The evaluator supports positional compound literals with implicit zero lanes,
unary `+`, `-`, and `~`, arithmetic and bitwise operators, valid lane shifts,
comparisons, lossless scalar broadcasts, scalar conditions, `_Generic`, and
`__builtin_choose_expr`. Numeric `__builtin_convertvector` queries convert each
lane using the target's scalar conversion rules. Conditions select values only
after both operands have passed source type checking. Evaluated object reads,
calls, and side effects do not become constants.

Integer lanes retain their width without scalar integer promotion. Unsigned
arithmetic wraps at that width, and comparisons produce all-zero or all-one
lanes. Signed overflow, division by zero, and invalid shifts are diagnosed,
including cases where Clang accepts an initializer after folding undefined
operations. Floating operations use target-format arithmetic, rather than host
floating-point arithmetic. GNU half and bfloat arithmetic with excess precision
remains unsupported; explicit lane conversions use their destination format.

| Static initializer expression | GCC 13 | Clang 18 |
| --- | --- | --- |
| Brace or compound-literal lanes | Supported | Supported |
| Unary plus, constant selection, `_Generic`, `__builtin_choose_expr` | Supported | Supported |
| Unary minus/complement, arithmetic, shifts, comparisons, broadcasts | Rejected by profile | Supported for defined operations |
| Identity `__builtin_convertvector` | Supported | Rejected by profile |
| Numeric `__builtin_convertvector` | Rejected by profile | Rejected by profile |

Storage reinterpretation casts, comma expressions, shuffle expressions, and
unspecified lanes remain outside the vector evaluator. Vector element extraction
has not been added to scalar constant evaluation. Accepted vector sizes remain
1, 2, 4, 8, and 16 bytes; larger vectors still require target-feature and ABI work.
Each fold bounds expression nesting to the existing 128-level limit and counts
at most 65,536 vector-expression and lane steps. Lane allocation checks the vector's
size and count first, including for caller-mutated translation units.

Retained analysis preserves the checked expression and initializer graph. It
uses the same evaluator and admission rules as ordinary analysis. No new vector
calling convention, Rust by-value ABI, or machine-code lowering is implied.

## Compiler versions and evidence

The source rules above follow upstream GCC 13 and Clang 18. Apple Clang 17.0.0
(`clang-1700.0.13.5`) accepts additional numeric-conversion static initializers.
That version difference remains an explicit compatibility gap; it is not used
to broaden all Clang profiles. LLVM's later
[constant-expression conversion change](https://github.com/llvm/llvm-project/pull/112129)
and the [current extension documentation](https://clang.llvm.org/docs/LanguageExtensions.html#builtin-convertvector)
cover newer Clang behavior.

The validation record includes 300 GCC/Clang source decisions, native O0/O2
storage-bit comparisons with UBSan, and cross-target LLVM outputs for half,
bfloat, long-double, binary128, and 128-bit integer lanes. Cross-target outputs
establish compiler acceptance and encoded constants, not native execution.
