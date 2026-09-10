# Validation

Validation is specific to a source revision, target, compiler profile, headers,
and generator configuration. Use the checks below on the revision being adopted.
The current [compatibility matrix](compatibility.md) describes supported forms;
it is not a certificate that every combination has passed native execution.

## Maintained checks

| Check | Entry point | What it establishes |
| --- | --- | --- |
| Unit and regression tests | `cargo test --workspace` | Expected behavior of covered syntax, semantics, preprocessing, and bindings. |
| Native compiler and FFI tests | `cargo test --workspace -- --include-ignored` | GCC/Clang comparisons and C/Rust calls on the configured host; external tools are required. |
| Real library bindings | [Upstream corpus](../corpus/README.md) | Constants, layouts, signatures, and actual calls for pinned zlib, SQLite, zstd, and libgit2. |
| Independent C acceptance | [Conformance](conformance.md) | Agreement on the eligible positive corpus; this alone does not test invalid-source rejection. |
| Malformed-input robustness | [Fuzzing](../fuzz/README.md) | Panics, sanitizer failures, timeouts, and selected internal invariants. |
| Downstream consumption | [Replacement readiness](replacement-readiness.md) | Build-script compatibility and representative runtime behavior for selected consumers. |
| Performance | [Benchmarks](benchmarks.md) | Generation cost on the measured workload, with output comparisons kept separate. |

The [CI workflow](../.github/workflows/ci.yml) runs native suites on Linux x86-64
and AArch64. Windows has package/all-features checks and separate native ABI
workflows. [macOS validation](../.github/workflows/macos.yml) runs on relevant
pushes to `main` and by request on PRs; Intel is opt-in. Cross-compilation proves
less than executing a C/Rust consumer on the destination platform.

## Results and artifacts

CI uploads reports, generated probes, and command logs as workflow artifacts.
Keep local outputs in ignored `corpus/results`, `fuzz/runs`, or
`benchmark-results` directories. Reports should identify the tested commit,
toolchains, input versions, configuration, and any omissions or accepted
differences. Preserve failed cases as small regression fixtures with a reusable
test; keep bulk logs and generated manifests out of source control.

For a release or adoption decision, link the relevant successful workflow runs
and summarize their scope in the release or PR. A historical pass does not
validate a later revision. Fuzz execution counts include rejected input and
do not measure how many programs had correct semantics or bindings.

## Historical results

The implementation-era captures remain available in Git history at
[the archive revision](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776):
[native/consumer reports](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence),
[fuzz campaigns](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/fuzz/evidence), and
[benchmark samples](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/benchmarks/evidence).
They are historical records, not dependencies of current validation.
