# Published callback stack CI

At 12:27 UTC on September 9, all eight audited workflows completed successfully:
seven on implementation commit `ec019da09675f0e27073fa74fc0a89b9ad25849e` and the
combined-head corpus run on `7db2b85027108be1515ad76263720e665cba2ab9`.
Seventeen jobs passed. The scheduled/manual fuzz campaign was skipped and did
not allocate a runner. Both PRs have empty labels; zero macOS jobs were allocated.

[Combined-head corpus](https://github.com/astral-sh/toucan/actions/runs/34349071817)
passed on Linux x86-64 and ARM, including the schema-3 analyzer build/tests and
binding comparisons. [Implementation CI](https://github.com/astral-sh/toucan/actions/runs/34349065986)
passed workspace tests and Rustdoc on both Linux architectures, Rust 1.96, lint,
and Linux/Windows package and allocator checks. Parent corpus, C acceptance,
Csmith, musl, Windows DLL consumers, and fuzz smoke workflows also passed.

The source audit proves equality of 624 Git blobs covering workspace crates,
Cargo/toolchain/configuration files, and all eight workflow definitions. The
combined head changes seven analyzer/script/documentation/evidence paths; the
parent run therefore covers the same workspace implementation, and the combined
head's own corpus run supplies direct validation of its changed tooling.

The Linux ARM log explicitly records `selected_bindings_compile_as_rust ... ok`
at 12:09:23 UTC. All 11 artifact metadata entries are retained and unexpired at
capture time. Artifact payloads were not downloaded. Intermediate pending
snapshots and an initial terminal-escape log-fetch refusal remain in the capture;
the successful retry wrote raw logs to a file and displayed only a sanitized
excerpt. No workflow was rerun/dispatched and no labels or repository state were
changed. The [summary](summary.json) and [capture](capture.json.gz) retain exact
run/job IDs, links, labels, Git identities, commands, source evidence and hashes.
