# toucan_preprocessor

A C preprocessor written in Rust. It expands macros, resolves includes, and
returns source ready for parsing. It runs in process without a compiler or
libclang; callers supply include directories, predefined macros, and any
in-memory resource headers.

```rust
use std::path::Path;
use toucan_preprocessor::{Config, Preprocessor};

let mut preprocessor = Preprocessor::new(Config::default());
let result = preprocessor.preprocess_str(
    Path::new("example.h"),
    "#define COUNT 4\nstruct Example { int values[COUNT]; };\n",
)?;
assert_eq!(result.expand_object_macro("COUNT")?.as_deref(), Some("4"));
# Ok::<(), toucan_preprocessor::Error>(())
```

## Inputs and output

Use `preprocess` for a file or `preprocess_str` for in-memory source.
`preprocess_files` accepts an ordered list: earlier headers act as forced
includes, and the last is the main file. Each call starts a fresh translation
unit with the configured macros.

`Preprocessed` contains expanded source, final macro definitions, filesystem
dependencies, and source-location mappings. `expand_object_macro` expands an
object macro against the final definitions; `resolve_location` maps an output
byte offset to its source token or macro invocation. File origins, macro
definition history, and documentation comments can also be captured through
`Config`.

## Configuration and support

The preprocessor handles object and function macros, variadic arguments,
stringification, token pasting, conditional directives, and quoted and
angle-bracket includes. It also supports extensions such as `include_next`,
`__has_include`, `__COUNTER__`, and `pragma once`.

Configure include paths and predefined macros for the target you are reading.
Compiler feature queries such as `__has_builtin` require a caller-supplied
`Config::feature_queries` catalog. The library does not discover host compiler
settings. For ready-made target and compiler profiles, use the
[`toucan` library](../../docs/library.md).

Set `Config::allow_filesystem` to `false` to restrict preprocessing to in-memory
source, forced includes, and virtual headers. Include depth, macro expansion
depth, token counts, and source/output bytes have configurable limits. Include and
macro expansion depths have a supported maximum of 256; the defaults are 64 and
128. Recursive preprocessing and final macro queries run on a scoped 16 MiB worker
stack shared with the parser. `with_preprocessor_stack` groups repeated standalone
calls into one worker session; worker-creation errors become diagnostics.

## Translation timestamps

`Config::timestamp` fixes `__DATE__` and `__TIME__` for the translation unit.
The library defaults to the Unix epoch: `"Jan  1 1970"` and `"00:00:00"`.
Set it with `PreprocessingTimestamp::from_unix_seconds` to use another UTC time.
The library never reads the clock or `SOURCE_DATE_EPOCH`; the
[CLI handles those defaults](../../docs/usage.md).

## Limitations

Unsupported input produces diagnostics. Known gaps include non-ASCII
identifiers, multicharacter preprocessing character constants, and `__VA_OPT__`.
Filesystem includes use the host's path conventions, even when the configured
target is another platform. Resource limits bound input and processing work;
they are not a process memory quota.

## Further reading

- [Include search](../../docs/include-search.md) and [header paths](../../docs/header-paths.md).
- [Compiler profiles](../../docs/compiler-profiles.md).
- [API definitions](src/lib.rs).
