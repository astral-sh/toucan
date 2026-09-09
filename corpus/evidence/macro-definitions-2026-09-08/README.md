# Written macro definition history

This capture validates optional ordered `#define` records on top of `f3450df`.
Entries preserve unexpanded definitions and physical/accessed input names after
later `#undef`, independently of the final macro environment and file-origin map.
The implementation checks its conservative retained-data budget before copying.

The preprocessor suite passes 72 tests, with nine native oracle tests left opt-in.
Four new cases cover definition chronology, forced/virtual input locations,
reset after failure, and budget exhaustion. Workspace and fuzz Clippy, public
Rustdoc, and five campaign-runner tests pass. A deterministic replay executes
720 preprocessing inputs: nine seeds across all 80 independent policy settings,
including capture enabled and disabled. This is not a sanitizer mutation run.

Nine real header routes compare a frozen prior binary with capture disabled,
history enabled, and history combined with declaration/file origins. All 36 runs
preserve preprocessed text and semantic units; eight Rust binding outputs are
also unchanged. Disabled capture has zero allocation-call or requested-byte
deltas. AWS-LC records 10,254 definitions and adds 21,408 allocation requests and
5,815,849 requested bytes for history alone. These counters measure allocation
traffic, not live memory. No latency improvement is claimed.

`summary.json` records sources, parent, checks, allocation deltas, and limits.
The archive contains raw logs, commands, helper sources, frozen binary hashes,
generated outputs and definitions, and every replay input. Its member hashes
are in `files.json`. The baseline control verifies that the prior library does
not expose the new capture API. The follow-up adapter decides how to evaluate
historical definitions; this layer leaves normal C expansion unchanged.
