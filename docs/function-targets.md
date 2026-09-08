# Per-function target options

Toucan retains supported GNU `target` attributes as function declaration properties.
They do not become C type qualifiers or change function-pointer compatibility.
The first supported set covers the MMX, SSE, and SSE2 wrappers in Clang's x86
intrinsic headers:

| Option | GCC on x86-64 Linux | Clang on x86-64 Linux, macOS, Windows |
| --- | --- | --- |
| `mmx`, `sse`, `sse2`, `no-mmx` | Supported | Supported |
| `lzcnt`, `bmi`, `bmi2` and their `no-` forms | Supported | Supported |
| `no-evex512` | Unavailable in the GCC 13 profile | Supported encoding restriction |

Unspecified features retain the compiler profile's baseline. This is a bounded
feature model, not a CPU selection API. `no-sse`, `no-sse2`, AVX settings, enabled
EVEX512, `arch=`, `tune=`, Arm settings, and function multiversioning remain explicit
unsupported diagnostics. Disabling SSE changes scalar ABI and lowering behavior
that this slice does not model. `min_vector_width` is a separate attribute and is
not silently ignored.

## Declarations and calls

`TranslationUnit::function_options` is a sparse map keyed by declaration index.
`FunctionOptions` exposes the effective target and inline annotations.
`FunctionTarget::options()` preserves ordered options, and `clang_spelling()`
preserves Clang’s identity-significant whitespace: `target("mmx")`
and `target("sse")` remain distinct even when the baseline enables both features.
An empty map allocates no storage. Query and binding entry points validate the
indices, targets, compiler restrictions, and option limits of caller-built units.

Clang also accepts trailing attributes on a function definition. GNU requires
attributes before the definition declarator; prototypes allow trailing attributes.

An unannotated redeclaration inherits earlier visible options. Repeated identical
options are supported. GCC also accepts permutations and duplicate spelling of the
same set of positive supported options. Clang distinguishes order, duplicates, and
whitespace when comparing target annotations. Other different histories require
unsupported GNU merging or Clang multiversioning. Empty attributes are ignored,
while their written occurrences remain available in checked results. For duplicate attributes on one declaration, Clang uses
the first and GCC uses the last. Clang ignores target attributes first encountered
after a definition, but still checks their argument syntax; adding such GNU
attributes remains unsupported. Clang inherits the first linked declaration even
when it appears in a block. Subsequent block declarations affect their lexical
scope; GCC can use them in a later file definition. The first-linked correction
has separate [native evidence](../corpus/evidence/function-target-first-linked-2026-09-08.json);
the original frozen measurement report remains unchanged.

Ordinary external calls and callbacks can cross function target boundaries.
Generated Rust retains their ordinary C ABI. Toucan does not attach Rust
`target_feature` attributes to external declarations or claim a Rust by-value ABI
for vector types. Native generated tests call target-annotated C functions and
pass Rust callbacks into C at GCC/Clang O0 and O2, including actual Rust 1.64.

MMX intrinsic requirements remain on `X86Intrinsic`. With `no-mmx`, Clang rejects
an evaluated MMX intrinsic; GCC permits explicit MMX builtins, including `emms`.
Definition parameter bounds use the function's target settings. Prototype bounds,
unevaluated operands, and proven discarded branches do not create evaluated
feature obligations.

Mandatory inlining has separate rules. Clang checks attributes visible on a
directly named callee during code generation. GCC's inlining can use annotations
encountered later, including a known function behind an explicit cast or address
operator. Toucan diagnoses proved feature mismatches in those cases. This does not
resolve arbitrary indirect calls, determine whether all calls survive optimization,
or provide machine-code generation.

The optional `lzcnt`, `bmi`, and `bmi2` features start disabled. Their ordered
positive and negative options change only the annotated function. In particular,
zstd's `target("lzcnt,bmi,bmi2")` dispatch wrappers do not enable global
`__BMI__`/`__BMI2__` predefines or select the separate intrinsic-header route.
This layer does not add the BMI intrinsic spellings themselves.

GNU later annotations need a sparse syntax catalog of potentially annotated
callees. Its bounded traversal includes nested block declarations and allocates
names only for written target attributes. Ordinary calls do not acquire a pending
record unless their visible options or a possible later annotation require one.
Clang continues to check only options visible at the call.

[The bit-manipulation target evidence](../corpus/evidence/bmi-targets-2026-09-08.json)
records native source/code-generation checks and the unchanged allocation counts
and requested bytes for four public-header binding workloads. This behavior follows
the [GCC 13 target attributes](https://gcc.gnu.org/onlinedocs/gcc-13.3.0/gcc/x86-Function-Attributes.html)
and [Clang target attribute](https://clang.llvm.org/docs/AttributeReference.html#target).

GCC keeps the first conflicting `noinline` or `always_inline` annotation across
accepted declarations; later conflicts produce compiler warnings. Clang retains
both when declared before a definition: `noinline` controls IR inlining, while
`always_inline` still imposes its direct-call feature check. Late Clang annotations
are ignored after argument validation. Toucan preserves both accepted flags and
their written spans rather than inferring that an annotated call must be inlined.

## Retained analysis

`CheckedCode::function_option_sites()` preserves each written attribute's arguments
and source span, plus the options visible at that declaration. Entity and body
lookups expose effective function settings. `inline_target_requirements()` links
feature-sensitive mandatory-inline calls to their callee, caller/callee options,
compiler checking stage, and whether a definition was available. A false
`definition_visible()` leaves body availability unresolved. Enclosing expression
and statement evaluation metadata still governs whether a call executes.

The sparse `function_options` and `inline_targets` JSON fields are additive to
inspection schemas 3 and 4, and omitted when empty. Attribute ranges refer to the
same mapped source as other checked occurrences. Limits cap per-function options
at 256, attribute arguments at 256 strings of 4 KiB each, and pending feature uses
and sparse function entries at 65,536; retained nodes and payloads also count
against `AnalysisOptions::limits`.

Compiler evidence uses assembly generation, since `-fsyntax-only` defers several
feature diagnostics. The distinction follows the
[GCC x86 function attribute rules](https://gcc.gnu.org/onlinedocs/gcc-13.1.0/gcc/x86-Function-Attributes.html)
and [Clang target attribute documentation](https://releases.llvm.org/18.1.8/tools/clang/docs/AttributeReference.html#target).
This slice does not establish general optimization or instruction-lowering equivalence.

## Validation and cost

The [validation report](../corpus/evidence/function-targets-2026-09-08.json) records
54 cases across four x86 compiler profiles, with separate syntax and assembly
results, native Rust calls/callbacks, and 385 retained seed/profile checks.
The unchanged Clang-preprocessed zstd sources advance to `min_vector_width`.

For 21 ordinary inputs across seven profiles, allocation counts and bytes match
base exactly within a parser session. Direct entry calls keep the same allocation
counts and add 96 bytes to the worker result packet. Metadata uses the existing
prior declaration index and allocates scope maps only when an annotation needs one.
The final short four-header timing batch measured roughly 0.9–1.8% overhead on
this shared host. The report retains the initial and final samples and executable
hashes; these measurements do not establish statistical equivalence.

## Minimum vector width

Clang's `min_vector_width` is a function optimization hint. Toucan preserves it as
`FunctionOptions::minimum_vector_width()`, in bits, and keeps each written hint
with its source span in `FunctionOptionSite::minimum_vector_width()`. It does not
change C type compatibility, enable instructions, or change generated Rust ABIs.
The value is a declaration hint: LLVM can compute a greater minimum from vector
parameter types or inlining. Toucan does not perform that lowering calculation.
Clang accepts the hint on all five targets; the pinned Arm backend probes do not
emit an LLVM width attribute.

The argument must be an integer constant expression whose original bit pattern
fits in 32 unsigned bits. For example, `(signed char)-1` requests 255 bits, `-1`
requests 4294967295, and `-1L` fails on LP64 targets. Zero and non-power-of-two
widths are accepted. The first hint on a declaration wins, but every argument is
checked. A later explicit declaration replaces an inherited hint. New hints after
a definition are ignored after validation. The first linked block declaration
can supply the inherited hint; subsequent block declarations have lexical scope.

Clang ignores width annotations on type-name operands after checking supplied
argument expressions. GNU ignores this unknown attribute, including its arity;
it still parses composite arguments and permits bare identifier arguments without
lookup. Such ignored arguments create any written types but produce no runtime
feature obligation. Their arithmetic is not evaluated as a width constant.

[Clang's attribute reference](https://releases.llvm.org/18.1.4/tools/clang/docs/AttributeReference.html#min-vector-width)
and [the pinned argument handler](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/clang/lib/Sema/SemaDeclAttr.cpp)
define these rules. Native diagnostics, emitted LLVM hints, inlining differences,
source hashes and ordinary/retained parity are recorded in the
[width-hint evidence](../corpus/evidence/minimum-vector-width-2026-09-08.json).
The [constant-expression probes](../corpus/evidence/minimum-vector-width-ice-2026-09-08.json)
also cover nonconstant operands in discarded branches. The
[integration report](../corpus/evidence/minimum-vector-width-integration-2026-09-08.json)
records the combined workspace suite and retained-graph seed checks after
declaration alignment and old-style definitions.
