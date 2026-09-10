# Generated binding correctness

`audit_generated_bindings.py` generates bounded C11 headers from a recorded seed.
It varies integer expressions and types, array bounds, scalar fields, nested
records, and unions. It then checks:

- GCC, Clang, and both Toucan profiles accept each original header.
- All three reject eight invalid mutations: duplicate members, negative arrays,
  conflicting typedefs, nonconstant enum values, oversized bitfields, incompatible
  function arguments, assignments to const objects, and incomplete record members.
- Toucan's emitted Rust compiles with `improper_ctypes` denied.
- Compiled Rust values, integer macro widths and signedness, record sizes and
  alignments, and field offsets equal independently compiled C observations from
  GCC and Clang at O0 and O2.

The C probes use the original header. Expected values and layouts never come from
Toucan or its binding report. The Rust probe refers to every expected constant,
record, and field, so missing or malformed output fails compilation. Tool crashes,
timeouts, empty rejection diagnostics, and unexpected exit codes fail the gate.
Regression tests inject an accepting-all frontend, a wrong emitted value, and
invalid Rust, and require the gate to reject each.

```console
cargo build --locked -p toucan_cli --bin toucan --no-default-features
python3 scripts/audit_generated_bindings.py --toucan target/debug/toucan \
  --seed 20260910 --count 16 --output corpus/results/generated-bindings
TOUCAN_ORACLE_BINARY=target/debug/toucan python3 -m unittest discover \
  -s scripts/tests -p test_generated_bindings_audit.py
```

Run on native GNU Linux with Python, GNU GCC, Clang, and Rust. Use `--gcc` and `--clang`
for explicit compiler paths, and `--toolchain ohm` for local Ohm Rust checks.
The output directory must be new. It retains all generated sources, bindings,
probes, compiler commands, diagnostics, observations, and a summary containing
compiler versions and the seed. Rerun that seed and count to reproduce the corpus.

The conformance workflow runs 16 headers with a seed derived from its run ID,
providing fresh cases while preserving reproducibility. CI retains artifacts for
14 days. This is a small, independent correctness gate for ordinary native C11
forms; it does not establish full C conformance, extension support, cross-target
ABI correctness, or function-call behavior.
