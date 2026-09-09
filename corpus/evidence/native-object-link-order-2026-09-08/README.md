# Native C fixture link order

The [native ARM failure](https://github.com/astral-sh/toucan/actions/runs/34272736557/job/102218123265) placed a C object after libc/system libraries. Its compiler-generated stack-guard reference could not resolve. The test helper now passes the object as a native static library before those libraries, preserving GCC stack protection and all compiler flags.

The archive reproduces 12 original ARM link failures and preserves 24 successful corrected QEMU executions with current Rust and actual Rust 1.64, plus 12 original-success controls. These are emulated ARM results; native ARM confirmation requires CI.

The root integration passes all six focused tests with current Rust and again with actual Rust 1.64, and workspace Clippy. FloatN, non-object pointers/callbacks and omitted conditionals use the shared helper. `root-integration.json` identifies the composed source; `summary.json` and `source.json` identify the independent frozen reproducer.
