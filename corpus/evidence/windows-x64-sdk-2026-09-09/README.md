# Native Windows x64 SDK headers

At commit [`678fb3e`](https://github.com/astral-sh/toucan/commit/678fb3e9b359a2baf5acaca583df02ae4f816c7b), [run 34384071509](https://github.com/astral-sh/toucan/actions/runs/34384071509) passed on a native Windows x64 runner with installed SDK `10.0.26100.0`. [Artifact 10117088369](https://github.com/astral-sh/toucan/actions/runs/34384071509/artifacts/10117088369) expires on October 9, 2026. The compressed [evidence](evidence.json.gz), [binding reports](binding-reports.json.gz), [verification summary](summary.json), and [manifest](manifest.json) retain the result after the artifact expires.

All three complete header inputs passed seven stages: MSVC C11 compilation, Clang C11 compilation, Toucan preprocessing, declaration/body checking, allowlisted Rust binding generation, native Rust compilation, and execution. The inputs were unchanged throughout the run. Every retained C object and Rust executable has x64 machine identifier `0x8664`.

| Input | Checked declarations | Generated types | Skipped declarations | Size/alignment pairs | Field offsets |
| --- | ---: | ---: | ---: | ---: | ---: |
| `basetsd.h` | 103 | 6 | 0 | 6 | 0 |
| `winnt.h` | 2,141 | 4 | 0 | 4 | 0 |
| `windows.h` | 6,840 | 7 | 0 | 7 | 5 |

The `winnt.h` wrapper supplies the `excpt.h`, `minwindef.h`, and `_AMD64_` prerequisites normally provided by `windows.h`. The `windows.h` wrapper selects `WIN32_LEAN_AND_MEAN=1`. Native Rust confirms the same expected sizes, alignments, and offsets as the C assertions. The independent archive review verified 54 command-log hashes and all three generated-binding hashes. SDK and compiler versions, tool hashes, include order, dependency hashes, commands, output hashes, and exit statuses are retained in the evidence.

The tested frontend supports the SDK's x64 `__ptr32` types with four-byte size/alignment, distinct type identity, and pointer conversions. Selected x64 `__ptr32` bindings receive an explicit diagnostic, including uses through typedefs, nested types, callbacks, and caller-owned types: native Rust pointers have an eight-byte ABI. The selected SDK layout types here do not require that unsupported representation.

This validates the recorded SDK, three header configurations, and selected layouts. Full Windows consumer builds, other SDK versions, and the complete Windows API surface remain unvalidated by this run. It does not exercise general Windows API calls; separate native DLL probes cover those recorded call cases.
