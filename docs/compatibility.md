# Compatibility

Toucan is an experimental C header frontend. The repository contains working
bindings and independent probes for substantial public APIs; it is not yet a
production-ready replacement for bindgen or a complete C compiler frontend.

## Supported scope

The current declaration pipeline handles common C11 declarations: scalar types,
pointers, arrays, typedefs, structs, unions, enums, prototypes, callbacks, variadic
declarations, external variables, integer constant expressions, and selected GNU
extensions.
Layout supports packing, explicit alignment, and bitfields. Binding generation has
additional representation constraints described below.

## Targets

The canonical target triples accepted by `--target` are listed below. The default
compiler ABI is used; flags such as `-fshort-enums` are not implied.

| Target | Layout model | Native validation |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | Implemented; GCC and Clang probes | [Four-library run passed](../corpus/evidence/native-2026-09-08.json) |
| `aarch64-unknown-linux-gnu` | Implemented; Clang cross-target probes | [Four-library run passed](../corpus/evidence/native-2026-09-08.json) |
| `x86_64-apple-darwin` | Implemented; Clang cross-target probes | [Four-library run passed](../corpus/evidence/native-2026-09-08.json) |
| `aarch64-apple-darwin` | Implemented; Clang cross-target probes | [Four-library run passed](../corpus/evidence/native-2026-09-08.json) |
| `x86_64-pc-windows-msvc` | Implemented; Clang cross-target probes | Not run; MSVC header syntax is incomplete |

The recorded [native run](https://github.com/astral-sh/toucan/actions/runs/34174435203)
passed on 2026-09-08, using GCC 13.3.0 on Linux and Apple Clang 17.0.0 on macOS.
The evidence identifies the tested commit and executable for each target. See the CI
results for the current commit; the historical run does not establish that later
changes pass.

## Current gaps

- Function bodies and variable initializers are parsed but not type-checked.
- Declaration constraints still need broader conformance testing. Prototype-local
  tags retain distinct identities and are checked against GCC and Clang.
- C++, K&R function definitions, variable-length arrays, TLS, atomic and complex
  types, unsupported calling conventions, and unknown ABI attributes are rejected.
- Extended floating-point types retain their identity but do not have supported
  layout or binding representations. `long double` has a target layout, but its Rust
  binding representation is not implemented.
- Large enum constants follow the target's GCC or Clang profile, including their
  types during and after the definition. Apple enum ranges requiring more than
  64 bits and values that cannot be represented without truncation are rejected.
- Rust bindings reject union bitfields, records containing bitfields passed by value,
  field-level alignment, and combined packing and explicit record alignment.
- Function-like macros and object macros that are not supported integer or ordinary
  string constants are reported as omitted. SQLite's `SQLITE_STATIC` and
  `SQLITE_TRANSIENT` destructor macros are examples. No invalid function pointer is
  synthesized to represent a sentinel.
- Macro expansion diagnostics identify an invocation location; they do not yet
  expose a full nested expansion backtrace.
- The bundled resource headers are fallbacks for common header declarations, not a
  complete C standard library or compiler SDK. Supply actual target resource headers
  and sysroots when required.
- The public API and JSON schema are experimental. No release stability commitment
  or broad compiler conformance claim has been made.

Unsupported selected ABI representations fail binding generation. Reports name
selected macros that cannot be emitted, and `--deny-skipped-macros` can make these
omissions an error. Internal-linkage declarations and function definitions are
reported as skipped. They are not exposed as callable externs. A function definition can still have an
exported symbol; generating bindings for definitions is outside the current scope.

## Recorded evidence

The [upstream corpus](../corpus/README.md) processes untouched public headers from
pinned zlib, SQLite, zstd, and libgit2 releases. The recorded native run emitted
1,648 declarations, skipped no selected declarations, and passed 4,086 C/Rust
comparisons on each of the four Linux and macOS targets. Actual FFI calls exercised
compression, SQLite queries, and libgit2 operations. The reports record 111 omitted
macros on Linux and 110 on macOS, with their reasons.

This run predates the bindgen API comparison and the all-record layout probes. Its
layout checks cover the 17 records and 88 field offsets named in the corpus probes.
New validation must be recorded separately against the version that ran it.

The selected API names, independent probe coverage, compiler versions, header
checksums, and commands are recorded with the result. Successful probes establish
the tested constants and representations. They do not validate every declaration
constraint or every possible call to those APIs.

The [dependency audit](../corpus/evidence/dependency-audit-2026-09-08.json) found no
known RustSec advisories in the workspace, fuzzing, or comparison tool lockfiles on
2026-09-08. It records the database revision and lockfile checksums; yanked package
status was not checked.

## Requirements for a production release

Complete the declaration and expression conformance work; replace or establish
reliable limits for pathological parser backtracking; extend the target/header
matrix; run sustained fuzzing and independent safety review; validate distribution
and allocator configurations; and define a stable API and diagnostics contract.
The [development plan](development.md) records the broader acceptance criteria.
