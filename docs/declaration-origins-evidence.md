# Declaration-origin capture evidence

This prerequisite records physical header paths and written file-scope declaration
occurrences for later binding selection. It does not enable the adapter's file
allowlists or rename callbacks yet.

The [compressed evidence](../corpus/evidence/declaration-origins-2026-09-08.json.gz)
records the exact composed baseline, helper/source/library hashes, commands, and
results. Validation includes 813 workspace tests (215 opt-in tests ignored), the
final preserved-directive regression and complete preprocessor suite (63 passed,
8 opt-in tests ignored), workspace Clippy, and Rustdoc with warnings denied.
A replay of 94 checked-code seeds across 11 compiler profiles and eight C modes
produced 5,529 matching successes and 2,743 matching diagnostics; it also checked
34,703 origin ranges and owner-local target links.

Default allocation counts are unchanged. The analysis worker's result adds a fixed
eight bytes, independent of declaration count (1, 100, and 1,000 tested); standalone
preprocessing adds none. Enabling both catalogs increased cumulative allocated
bytes by 0.69–1.69% on eight primary GNU/Clang header routes and the AWS-LC wrapper.
These are allocation requests, not peak memory measurements. Single-run timings
are retained as observations on a shared host, without a speed claim.

The eight generated binding files, all nine semantic Units, and all nine expanded
inputs were byte-identical before/after and with capture enabled. The unchanged
AWS-LC 0.44.0 crypto wrapper produced 4,455 declarations and 5,055 origins; all
origins resolved to physical files. This proves source/provenance parity, not an
AWS-LC binding replacement or native cryptographic execution.

A pinned bindgen 0.72.1/libclang18 reference rejects a reference-only `sizeof` use
as a header selection root. The catalog therefore distinguishes tag references
from new declarations and standalone redeclarations. Macro-definition origins
retain the final active environment; the adapter's separate first-definition
history policy is follow-up work.
