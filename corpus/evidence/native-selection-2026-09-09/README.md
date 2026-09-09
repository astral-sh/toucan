# Native validation after declaration selection changes

All 1,258 native workspace tests pass across 270 Cargo test and doc-test groups,
including the normally ignored compiler-oracle tests. Rustdoc passes with warnings
treated as errors. The capture uses the frozen `00ec563` source; all 2,359 files
remain unchanged, with no additional files. Its complete Git tree is identical
to published `b371cd0` in PR #293.

The checked frontend target completes a five-minute AddressSanitizer campaign
with 35,712 executions and no findings. It covers optional object values,
parameter-type dependencies, documentation origins, and the declaration-type
comparison budgets. Peak reported RSS is 631 MiB. Mutation counts include
rejected inputs and do not establish C conformance or complete memory safety.
LeakSanitizer was disabled in the ptrace environment. Builder emission is outside
this checked-target campaign.

The [capture](capture.json.gz) retains the complete source manifest, test logs,
commands, sanitizer build and run logs, dictionary, and stack replay mapping.
The [initial corpus](initial-corpus.tar.gz) and its hashes reproduce the starting
inputs. The [summary](summary.json) records counts, resource use, and artifact
hashes. The earlier CLI integration failure and its source revision are preserved
in the capture; the corrected source passes the complete suite.

The [GitHub capture](ci-capture.json.gz) records all seven successful workflows
on the identical published tree: fifteen successful jobs and one optional fuzz
campaign skipped. Both Linux native suites and corpus jobs pass, including the
corrected ARM compilation fixture. Windows packaging and DLL consumers also pass.
PR labels are empty and no macOS runner was allocated.
