# Origin and conformance sanitizer checks

Both bounded AddressSanitizer campaigns used an immutable worktree at `a215c7e`. Its production code includes non-object queries, omitted conditionals, floating-name constraints and declaration-origin capture. Checked analysis retains and validates origin ranges and target IDs; preprocessing validates physical mappings and macro locations.

The checked campaign ran 46,217 inputs in 301.30 seconds, adding 1,712 corpus entries, with peak RSS 633 MiB. Preprocessing ran 500,395 inputs in 181.10 seconds, adding 6,173 entries, with peak RSS 513 MiB. Both completed with no findings and unchanged source manifests. These are bounded runs, not a proof that all inputs are safe.

The reports preserve toolchain and sanitizer identities, commands, limits, selectors, initial inputs, source hashes, logs and corpus results. Native fixture-link changes and later adapter/optimization work are separate commits and are not covered by these source manifests.
