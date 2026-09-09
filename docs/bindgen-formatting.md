# Bindgen adapter formatting and output

The adapter supports the `Formatter::None` and `Formatter::Rustfmt` variants
available in bindgen 0.72.1 with `default-features = false` and `runtime` enabled.
Rustfmt is the default. The optional `Prettyplease` variant and feature are not
implemented. String parsing accepts only `none` and `rustfmt`; unsupported names
return an error.

`Builder::generate()` performs frontend and binding generation work without
running rustfmt. Each `Display`, `write`, or `write_to_file` call formats the
generated declarations. This preserves bindgen's timing: `RUSTFMT` is read when
the output is written, and a subsequent write runs the formatter again.
Generation callbacks remain on their existing thread. A scoped writer thread
feeds rustfmt's stdin while its stdout is drained, so neither pipe must fit the
entire module in its buffer.

The executable selection is `with_rustfmt(path)`, then a Unicode `RUSTFMT`
environment value, then `rustfmt` on `PATH`. Paths and arguments are passed
directly to the process. `rustfmt_configuration_file(Some(path))` adds
`--config-path`; both `Some` and `None` enable rustfmt. A non-Unicode configuration
path is omitted, matching the reference implementation. The Rust target selects
edition 2021 through Rust 1.84 and edition 2024 from Rust 1.85 onward. The legacy
`rustfmt_bindings(bool)` method selects Rustfmt or None.

Raw lines are written after Toucan's banner and before declarations. Their
contents, including internal newlines, remain byte-for-byte unchanged and are
excluded from formatting. A host line ending follows each raw line, followed by
one blank line. The adapter removes only the exact raw-line suffix produced by
the core emitter; core library and CLI ordering are unchanged.

## Error behavior

`Bindings::write<'a>(&self, Box<dyn std::io::Write + 'a>)` accepts borrowed writers
and propagates their errors, including errors after a partial write.
`write_to_file` creates or truncates its destination and delegates to `write`.
Formatter failures do not turn generation or writing into an error:

| Formatter result | Output |
| --- | --- |
| Exit 0, valid UTF-8 | Formatter stdout, including empty output |
| Exit 3, valid UTF-8 | Formatter stdout and a warning |
| Exit 2 | Diagnostic and unformatted declarations |
| Other unsuccessful exit or launch failure | Diagnostic and unformatted declarations |
| Invalid UTF-8 stdout | Unformatted declarations |

Invalid UTF-8 is handled before the exit code, as in bindgen 0.72.1. A formatter
that closes stdin early does not turn an otherwise successful exit into an
error. Formatting provides source presentation; it does not validate
caller-supplied raw lines or establish ABI compatibility.

`clang_version()` exposes bindgen's `ClangVersion` field types while identifying
the actual frontend. `full` is `Toucan <package version> (no libclang)` and
`parsed` is `None`. A selected Clang semantic profile is not a native libclang
installation.

## Validation

The [saved Linux observations](../corpus/evidence/bindgen-formatting-2026-09-08.json.gz)
compare 24 behavior pairs against pinned bindgen 0.72.1 using the same Rust
consumer source. They cover default and explicit formatters, deferred and
repeated writes, executable precedence, writer errors, configuration activation,
Rust-target editions, each fallback path, empty output, and early stdin closure.
A formatter that writes 256 KiB before reading more than 64 KiB of input exercises
concurrent pipe handling. Both generators' actual rustfmt output compiles with
rustc for Rust-target 1.64 and 1.85 syntax. This is a current-rustc syntax check,
not an additional minimum-version runtime claim.

The same probe compiles the borrowed writer signature, checks all supported
formatter names and rejects unsupported names. Its version comparison records
the intentional native-Clang versus Toucan identity difference. Binding source
bytes, generator banners, and declaration representations are not asserted to be
identical by this formatting test.

Focused Rust tests cover raw-line ordering, file truncation, partial writer
errors, deferred subprocess execution, invalid formatter output, and real
formatted-output compilation. The real formatter test requires `rustfmt` and
`rustc` on `PATH`; run it with:

```sh
cargo test -p toucan_bindgen --test formatting -- --include-ignored
```

These probes do not replace the pinned AWS-LC and uv generated-binding consumer
gates. No native Windows or macOS formatter process run is claimed here.
