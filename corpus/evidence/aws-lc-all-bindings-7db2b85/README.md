# AWS-LC all-bindings validation

Frozen frontend: `7db2b85027108be1515ad76263720e665cba2ab9`.

The pinned AWS-LC 0.44.0 reference and Toucan builds both enable `all-bindings`,
`bindgen`, and `prebuilt-nasm`; SSL stays disabled. Both compile the unchanged
upstream wrappers, run 98 generated layout tests, and retain the same 41 crypto
artifacts as the prior [crypto-only capture](../aws-lc-builder-7db2b85/summary.json). Six independently C-compiled layouts
and a 23-byte C/Rust memory-BIO round trip agree. The BIO check calls the upstream
all-bindings `BIO_get_mem_data` Rust wrapper and the C macro.

Cargo feature, dependency and dep-info audits verify the fresh generated output
under `use_bindgen_pregenerated`. All nine frontend crates originate at the frozen
snapshot, whose 2,389 files remain exactly unchanged. The existing 13-package,
15-edge consumer graph remains the same after excluding the generator.

The archive retains adapted fixture/runner sources, raw Cargo logs, generated
bindings, C/Rust outputs, source inventories, and the post-run audit. Large native
executables remain in their recorded cache paths with hashes. Warm targets are
reused; only the sources and evidence directories are fresh. Validation duration
and shared-host disk endpoints are not performance or peak-memory measurements.
This proves the recorded native Linux profile, not SSL, another target, complete
public API equality, or every additional type's C ABI. The six independent C
layout controls and the 98 generated Rust layout tests are separate checks.

The post-run raw-artifact review was performed by the author of this capture;
this evidence does not claim a separate reviewer.
