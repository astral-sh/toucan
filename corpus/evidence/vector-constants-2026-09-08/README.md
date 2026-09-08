# Vector constant validation

`summary.json` identifies the implementation files and validation scope.
`compiler-oracles.json.gz` contains each exact C source, command, compiler version,
classification, and diagnostic. Its 300 decisions cover C11/GNU11 on native
GCC and Clang targeting the five physical targets. Format LLVM outputs are
cross-target compiler evidence; native lane comparisons are separate.

The final ASan campaign archives its original corpus, source hashes, runner
configuration, build log, and fuzzer log. It used the existing checked fuzzer
with all seven profiles and both language-mode seed variants. The separate
ASan executable also queried the public vector encoder and used snapshots after
their source environments were dropped. Leak detection was disabled on this
ptrace-based devbox.

The archived query harness, Cargo manifest, and lockfile are the exact inputs to
that executable. Their absolute paths identify the original worktree and scratch
directory. To replay elsewhere, change the manifest dependency and the harness's
`checked_invariants` and seed paths to the desired checkout, then run Cargo with
nightly Rust, `RUSTFLAGS=-Zsanitizer=address`, and an explicit native target.
The ordinary/native vector tests live in `crates/toucan_semantic/tests/vector_constants.rs`.

The first exploratory probes include deliberately undefined vector operations
and compiler-accepted forms outside this implementation. They record compiler
behavior and do not widen the supported evaluator or ABI contract.
