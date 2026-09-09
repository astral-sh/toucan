# Csmith fixtures

These four unmodified programs exercise scalar control flow and two- and
three-dimensional static addresses, including arrays of qualified record pointers.
Their source hashes, Csmith options and runtime-header hashes are pinned in
[manifest.json](manifest.json). The generated comments retain the original commands;
those historical output paths are not used by the harness.

The four runtime headers come from Ubuntu's Csmith 2.3.0 package, whose URL and
SHA-256 are also pinned. Their upstream copyright and BSD license remain intact;
see [runtime/LICENSE](runtime/LICENSE). The selected native Linux configuration
uses only these headers. Other Csmith configurations may need additional upstream
runtime headers, supplied with `--headers`.

No generator installation is needed to check the fixtures:

```sh
cargo build -p toucan_cli
cargo build -p toucan --example audit_csmith
python3 scripts/audit_csmith.py --output /tmp/toucan-csmith \
  --toucan target/debug/toucan \
  --runner target/debug/examples/audit_csmith
```

The output directory must be new. The driver verifies all pinned files, requires
both native compilers to accept each fixture under strict C11, checks each
compiler's unchanged `.i` output with the matching Toucan profile, and compares
normal/retained declaration hashes. One fixture runs with GCC and Clang at O0/O2
and O1 with UBSan; all six executions must agree. The Linux CI job uses this gate.
Mac and Windows execution are not covered by this harness yet.

## Larger corpora

`--sources DIRECTORY` checks untouched generated `.c` files without invoking
Csmith. `--csmith EXECUTABLE --count 100 --runtime-count 16` generates a fresh
bounded corpus using the manifest's options, starting at seed 2026090801. Supply
`--headers` for another runtime. Generator and version commands run inside the
report directory so Csmith's `platform.info` stays outside the checkout.

Compiler-rejected generated sources are explicit exclusions. They do not count
as accepted frontend inputs, and an audit with no eligible sources fails. For the
pinned CI fixtures, any exclusion also fails the gate. Crashes, timeouts, output
limits, preprocessing failures, frontend rejections, incomplete declaration
output and CRC/sanitizer failures produce separate evidence and a nonzero exit.
Native runtime agreement is an oracle check; Toucan does not execute these programs.

Child commands have time and output limits. Sources are capped at 4 MiB, corpora
at 10,000 files, and native runtime controls at 100 programs. Programs execute as
the invoking user; the harness provides resource limits, not process isolation.
All successful declaration comparisons hash the complete output, capped at 256 MiB.
Only compiler-generated Clang line-marker diagnostics are suppressed when
rechecking `.i` files: original-source eligibility remains strict, and Toucan
receives the original markers.

## Recorded 100-seed audit

On native x86-64 Linux with GCC 13.3 and Clang 18.1:

| Result | dbda83c | 671e9c7 |
| --- | ---: | ---: |
| Strictly eligible generated sources | 58 | 58 |
| Sources accepted in both frontend profiles | 50 | 58 |
| Accepted profile results | 100/116 | 116/116 |
| Normal/retained mismatches | 0 | 0 |
| Crashes, timeouts or pipeline failures | 0 | 0 |

The other 42 generated sources, plus the original unbounded control, were
excluded for pointer constraints. All eight frontend failures were the
multidimensional static-address initializer gap fixed by 671e9c7. The replay used
the exact original inputs and native eligibility results; previously accepted
declaration outputs stayed identical.

Sixteen eligible sources completed 96 GCC/Clang O0/O2 and O1+UBSan executions with
equal per-source CRCs, including 32 clean UBSan runs. This does not prove absence
of undefined behavior in every generated program and is not a throughput result.
The compressed [baseline](../../evidence/csmith-dbda83c-2026-09-08.json.gz) and
[replay](../../evidence/csmith-671e9c7-2026-09-08.json.gz) retain compiler identities,
source/dependency hashes, commands, exclusions, diagnostics and every runtime result.
The baseline also retains the initial rejected Clang marker rechecks; the final
summary uses the explicitly documented metadata exception.

The [integration report](../../evidence/csmith-integration-2baa121-2026-09-08.json)
records a fresh four-fixture gate after the function-target layer and independent
verification of 101 source files, 116 preprocessed inputs, 432 declaration outputs,
and the 96 native execution artifacts in the larger audit.
