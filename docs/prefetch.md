# Data prefetch

`__builtin_prefetch` returns void and accepts a data address followed by optional
read/write and locality hints. The default hints are read (`0`) and high locality
(`3`). Both compiler families advertise the builtin through `__has_builtin`.

The address is converted to `const void *`. GNU declarations that preserve its
builtin identity can specify different qualifications on `void *`; those written
parameter conversions remain visible. Optional arguments retain C's default
argument promotions. A `long long` constant therefore remains `long long`, rather
than being silently truncated to `int`.

Clang permits at most three arguments and requires integer constant expressions
in ranges `0..=1` and `0..=3`. GNU accepts additional arguments, evaluates their
side effects, and checks its first two hints after inlining and folding. GNU
replaces an out-of-range lowered constant with zero after issuing a warning.
`PrefetchHint` exposes the defaults, ranges, compiler stage, and fallback. A
successful GNU source analysis does not discharge an unresolved lowering check.

Retained calls use `Builtin::Prefetch` and preserve every supplied operand.
GNU variadic packs retain their expansion marker; hint positions and omitted
defaults apply after that expansion.
`CheckedCode::prefetch_arguments` also recognizes GNU indirection, comma/statement results, and selected generic
or choose-expression function designators. It does not resolve escaped variables, explicit address-taking, or
casts. GNU can recover an escaped builtin pointer during optimization, while
explicit address/cast forms of the direct designator are rejected by GNU source checking. The original callee
tree remains available to consumers.

The cache hint does not read or write a C object, establish synchronization, or
make a volatile or atomic access. A backend can omit the cache instruction, but
must preserve argument evaluation. Invalid prefetched addresses do not themselves
fault; evaluating an invalid address expression still can. Relative argument
evaluation order remains unspecified by C.

GNU function values retain `BuiltinFunction::Prefetch`, denoting the external
`__builtin_prefetch` symbol. A link using an escaped address needs a definition of
that symbol; no C library implementation is implied. Clang requires direct calls.
Compatible source declarations preserve their entity and symbol override. Clang
asm renames are accepted by its frontend but fail in its pinned backend; Toucan
retains both facts rather than inventing a replacement lowering. The shared
limitation on asm declarations nested in unevaluated operands remains explicit.

Clang's syntactic side-effect query treats this builtin as const and still
examines its operands. That classification does not make a prefetch call a
foldable C constant. GNU and Clang retain their existing different rules for a
void `__builtin_constant_p` operand.

## Validation and compiler phases

The source contract follows [GCC 13's builtin documentation](https://gcc.gnu.org/onlinedocs/gcc-13.1.0/gcc/Other-Builtins.html)
and [Clang 18's language extensions](https://releases.llvm.org/18.1.8/tools/clang/docs/LanguageExtensions.html#builtin-prefetch).
LLVM defines the eventual [prefetch instruction hint](https://releases.llvm.org/18.1.8/docs/LangRef.html#llvm-prefetch-intrinsic).

Native tests distinguish syntax decisions, code generation, and execution.
They check observable address/extra-argument effects and unchanged object contents
at C `-O0` and `-O2`. Cross-target checks make no native execution claim.
Clang 18 accepts `long long` hint constants but emits an LLVM call with mismatched
integer widths; LLVM 18's verifier rejects that module. Toucan keeps the valid
source types and records this as a compiler lowering defect.

Both pinned compilers interpret the low 64 bits of a wider constant when checking
these hints. `PrefetchHint::normalized_value` exposes that narrow implementation
rule while the expression keeps its original type and value. This is specific to
prefetch hints, not an ordinary C conversion. Clang's assertion-enabled checker
and its LLVM output have separate limitations for these wide arguments.

GNU keeps a direct-designator restriction through indirection, `_Generic`, and
`choose_expr`. Its comma and statement expressions instead produce pointer values,
which can be used in comparisons and boolean contexts. A private expression category
preserves this distinction without treating all escaped pointers as builtins.

The [validation record](../corpus/evidence/prefetch-2026-09-08.json.gz) includes
compiler commands, source and binary hashes, exact graph comparisons, native
effect checks, and allocation measurements. The five unchanged zstd inputs are
regression controls. Their acceptance predates this layer.

The integrated layer passes 775 workspace tests and 384 native compiler decisions
across the four language modes. Sixteen native executables preserve argument
effects at `-O0` and `-O2`. All 3,872 checked seed pairs agree, and eight existing
binding outputs are byte-identical. The retained-graph ASan campaign completes
26,563 inputs in 181 seconds with no findings.

Seven real source files also pass fresh preprocessing with explicit GNU 13.3 and
Clang 18.1.3 version overrides in both ordinary and retained analysis. All 14
declaration pairs agree. SQLite requires the library audit's larger explicit
retention budget; the CLI's default edge-limit rejection is preserved in the
[integration evidence](../corpus/evidence/prefetch-integration-2026-09-08/summary.json).
These runs do not change the default compiler version macros or establish
optimizer or machine-code equivalence.
