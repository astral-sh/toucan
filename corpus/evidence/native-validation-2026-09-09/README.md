# Native validation refresh

The frozen `acfb815` source passed all 1,215 workspace tests across 259 Cargo test
and doc-test groups, with no failures, ignored tests, or filtered tests. The run
included native compiler and FFI probes using `--include-ignored`. Counts use the
last result in each Cargo group, excluding nested child harness summaries.
Rustdoc also passed with warnings denied.

All 2,301 committed source files still match that Git tree. A concurrent native
helper generated one Python bytecode file; its path and hash remain recorded.
Rustdoc output was moved out of the frozen source directory after completion.
Neither generated artifact changed an original source input.

The captured GitHub runs passed Linux x64 and ARM native tests, both native
corpus jobs, Linux and Windows packaging, lint, Rust 1.96, GCC and Clang C11
acceptance, both musl targets, Csmith, and fuzz smoke testing. They ran at
`14ad623`, whose frontend crates are byte-identical to `acfb815`. The scheduled
fuzz campaign was skipped. These runs contain zero macOS jobs.

The checked-analysis fuzz target at `02a68ee` completed 43,501 executions in
301.133 seconds with AddressSanitizer and no findings. Peak RSS was 621 MiB. This
target now checks optional object values and documentation origins alongside the
checked representation. The run uses 11 target profiles, eight C language modes,
a 16 KiB input limit, a five-second input timeout, and a 1 GiB RSS limit.

The [summary](summary.json) identifies exact revisions and checksums. The
[capture](capture.json.gz) preserves the source inventories, complete test and
sanitizer logs, tool versions, commands, corpus inventories, and GitHub job
records. The [initial corpus](initial-corpus.tar.gz) and captured dictionary allow
replay. The copied sanitizer executable remains at the recorded local path, with
its checksum verified after the run.

Execution counts include rejected inputs. This campaign covers semantic metadata;
it does not exercise Builder comment attachment and emission. LeakSanitizer was
disabled in the local ptrace environment. Passing these checks does not establish
complete C conformance or complete memory safety.
