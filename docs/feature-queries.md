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

The infrastructure layer does not change the facade's advertised feature catalog.
Connecting the semantic builtin and attribute classifiers is a separate change.
Other query operators still require their own grammar and feature policies.

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
