# Native validation at 7db2b85

The frozen combined implementation passes 1,268 workspace tests with native
compiler probes enabled. All 263 planned test binaries and ten doctest groups
complete with no failed, ignored, or filtered tests. Rustdoc passes with warnings
denied, and all 43 Python validation tests pass. All 2,389 source files retain
their exact Git-blob and SHA256 identities.

The [capture](capture.json.gz) preserves the logs, Cargo test-binary inventory,
workspace metadata, commands, environment, source manifest, and completion audit.
The original local test process's numeric exit status was unavailable after its
tool session ended. We verified every planned binary and doctest group against
its final result and confirmed the process had finished; the capture does not
invent an exit code. The independent GitHub workspace jobs also pass.

## Binding generation checks

Three completed AddressSanitizer campaigns exercise both callback projection
policies. Each runs for approximately 301 seconds with an 8 KiB input limit,
five-second per-input timeout, and 1 GiB memory limit.

| Source | Implementation covered | Executions | Peak RSS |
| --- | --- | ---: | ---: |
| `d2fdcc0` | Nullable function typedefs | 40,037 | 655 MiB |
| `f7f1827` | Nullable typedefs and alias-cycle checks | 27,346 | 664 MiB |
| `7db2b85` | Combined implementation, including recursive work limits | 26,082 | 674 MiB |

All three complete without findings and preserve their source manifests. Each
subdirectory contains the original report, full logs, dictionary, harness,
source and binary hashes, and initial corpus archive. The later campaigns reuse
prior corpus entries; their execution counts are not performance comparisons.
LeakSanitizer was disabled in the ptrace environment. These bounded campaigns do
not prove complete safety, compile emitted Rust, or establish C conformance.
The capture also preserves the earlier 228 native binding-test results.

## Published CI

The [CI audit](ci/README.md) records eight successful workflow runs and 17
successful jobs. The optional scheduled/manual fuzz job is skipped. Allocations
are Linux x64, Linux ARM, and Windows; there are no macOS jobs, and the inspected
PRs have no labels requesting them.

Workspace CI ran at `ec019da`. Its 624 crate, Cargo, toolchain, configuration, and
workflow Git blobs are identical to `7db2b85`. The latter adds the enum analyzer
and comparison changes and passes its own corpus workflow on both Linux
architectures. This distinction is retained in the raw run and job metadata.

The [summary](summary.json) records counts, artifact hashes, and scope limits.
Real-header FFI, application consumers, and performance are separate captures.
No new macOS execution or general drop-in compatibility is claimed here.
