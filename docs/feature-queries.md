# Preprocessor feature queries

The standalone preprocessor accepts an optional `FeatureQueries` configuration
for `__has_builtin` and `__has_attribute`. Its immutable `FeatureQueryProvider`
supplies supported names and numeric results. The preprocessor does not depend on
target or semantic crates. A missing configuration leaves both operators undefined.

Queries are predefined operators with compiler-specific argument rules. GNU
expands builtin arguments; Clang reads the identifier as written. Both expand
attribute arguments. GNU accepts an optional attribute namespace; the provider
receives its spelling. In GCC 13, `gnu::aligned` is recognized in GNU11 but returns
zero in C11, so a frontend's catalog must also carry its language mode.

The operators rescan inside ordinary source, conditions, aliases, and wrappers.
Malformed arguments diagnose even on the unevaluated side of `0 && query()`;
inactive directive groups remain unprocessed. Argument expansion and results use
the existing token, byte, and depth limits. Result tokens retain invocation locations.

`#undef` and `#define` can remove or replace an operator. `Config::undefine`
provides the corresponding command-line operation. Each entry point restores the
configured initial state. `Preprocessed::is_defined` includes active operators;
`expand_object_macro` observes their final state when expanding another macro.
Operators do not appear as fake replacement strings in the macro map.

The provider must implement bounded, deterministic queries. An `Arc` shares its
immutable catalog between configuration copies; the preprocessor does not allocate
a per-name cache. Numeric results permit standard-version dates such as GCC's
`__has_attribute(fallthrough) == 201910`.

The facade connects these two operators to the semantic builtin and attribute
classifiers. The catalog carries the compiler, target, and language mode independently
of identity macro overrides. Unknown names and known attributes whose constraints
are ignored return zero. A positive result still requires valid operands, a supported
target feature, and a representable ABI at a Rust call boundary. GCC's accepted
parser forms `__builtin_complex` and `__builtin_va_arg` remain unadvertised, matching
its query registration. CLI and Builder `-D`/`-U` overrides apply to operators too.

The other five query macros still return zero; they need their own grammar and
feature policies. Compiler version macros have not changed in this layer.

Native differential tests cover 22 cases in C11 and GNU11 with native GCC and
Clang's five original targets: 264 decisions, including successful output values,
argument effects, and syntax rejection. The standalone tests also cover resource
limits, final environments, configuration overrides, entry-point resets, and source
locations. Cross-target preprocessing uses no sysroot and establishes no ABI claim.

The preprocessing fuzzer selects GNU/Clang query rules and trigraph defaults
independently. Its small test catalog exercises recognized and unknown names;
the production frontend's supported-feature catalog remains separately owned.

The [recorded differential run](../corpus/evidence/feature-query-operators-2026-09-08/summary.json)
also includes the extracted GNU AArch64 compiler: all 308 case/profile/mode
decisions and successful outputs match. Raw commands, versions, sources, results,
and the capture driver are retained. GNU namespace availability in C11 is recorded
explicitly rather than inferred from the GNU11 result.

## Semantic catalog evidence

The [catalog run](../corpus/evidence/feature-catalog-2026-09-08/summary.json) compares
486 builtin and attribute names across seven profiles and two language modes:
6,804 availability comparisons with no positive claim for a native-unavailable
feature. This measures advertised availability, not complete native feature equality.
Selected families also parse and type-check in ordinary and retained analysis;
invalid operands and attribute arguments still diagnose.

All four default header bindings are byte-identical to the previous recorded
outputs. Seven project source files pass ordinary and retained analysis through
fresh Toucan preprocessing and frozen earlier compiler-preprocessed controls
(28 runs). These use GNU11 analysis; libgit2's original C90 flags remain a separate
gap. The unchanged zstd build script passes all four feature configurations, with
four consumed bindings and 25 identical runtime artifacts.

Seven paired warm CLI measurements per header range from 0.6% faster to 2.1%
slower than `d7ba4de` on this shared host. Configuration and parsing allocate less;
binding generation allocations are unchanged. These measurements establish no
speed improvement or in-process comparison against bindgen.

The archive also records a 120-second AddressSanitizer preprocessing campaign at
`d7ba4de`: 415,264 executions with no finding. It uses the mechanical operators'
small test catalog, independently of this semantic catalog. Leak detection was
disabled in the ptrace environment.
