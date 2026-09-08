# Selective enum reference capture

`selection-evidence.json.gz` preserves thirteen invocations of bindgen 0.72.1,
their full source, commands, output and diagnostics, and binary/input hashes.
The reference frontend is `/home/dev-user/.cache/toucan/tools/bin/bindgen` with
`LIBCLANG_PATH=/usr/lib/llvm-18/lib`; the probe uses no system includes. This is
source-generation evidence, not an execution of the generated bindings.

Canonical C tags and first anonymous typedef names select their enum. Later
aliases do not. A truly unnamed enum can match an enumerator. The lexical nested
case selects `Holder_Nested` in bindgen, while Toucan's bounded selector uses C
names and does not claim that generated-name compatibility.

Run `probe_selection.py` with `selection.h` beside it to regenerate the reference
capture. Its source paths identify the original local capture; adjust `BINDGEN`
and `LIBCLANG_PATH` when reproducing elsewhere. Repository Rust tests separately
check generated bindings and actual GCC/Clang C-to-Rust enum calls.
