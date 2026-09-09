# Use the library

Use [`crates/toucan`](../crates/toucan) as a Cargo path dependency. This example
preprocesses an in-memory header with filesystem access disabled and generates bindings:

```rust
use std::path::Path;

use toucan::{BindingOptions, Config, Target};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.allow_filesystem = false;

    let compilation = toucan::parse_source(
        Path::new("api.h"),
        "typedef struct api_point { double x, y; } api_point;",
        &config,
    )?;
    let (bindings, report) = compilation.bindings(&BindingOptions {
        allowlist: vec!["api_*".into()],
        ..Default::default()
    })?;

    assert!(report.skipped_declarations.is_empty());
    println!("{bindings}");
    Ok(())
}
```

`Compilation::unit()` exposes the declarations and types for other consumers.
Set `Config::analysis.retain_code` to retain typed bodies, expressions, initializers,
and source references through `Compilation::checked()`. See the
[analysis API](analysis-api.md) for ownership, runtime bounds, and source locations. Use
`parse_file` for files, and configure include directories, predefined macros, virtual
headers, and resource limits through `Config::preprocessor`.

## Process and resource policy

The integrated frontend and binding adapter do not invoke a C compiler. The
standalone `toucan_parser::parse` compatibility entry point is an exception: it
launches its configured C preprocessor; `parse_preprocessed` accepts text without
that process. Library crates forbid unsafe Rust and leave allocator selection to
the embedding application.

Library configuration defaults to the Unix epoch for `__DATE__` and `__TIME__` and
never reads a clock or `SOURCE_DATE_EPOCH`. Set `config.preprocessor.timestamp`
with `PreprocessingTimestamp::from_unix_seconds` to choose another value; see
[translation timestamps](../crates/toucan_preprocessor/README.md#translation-timestamps).

See [parser limits](parser-limits.md) for resource budgets and embedding APIs,
and [architecture](architecture.md) for the individual library crates.
