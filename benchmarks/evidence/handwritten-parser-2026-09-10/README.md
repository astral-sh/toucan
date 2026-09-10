# Handwritten C parser

This comparison replaces the generated parser at `f1e8dc89acc74675cf81475fdc3402a76a3d09e9` with the handwritten parser introduced in `0dded60`. The lexer, declaration parser, statement parser, and expression parser are new. The AST and semantic interfaces retain their existing representation; no generated parser or alternate parser backend remains.

## Compatibility

The four real-project workloads use untouched zlib 1.3.1, SQLite 3.45.1, zstd 1.5.7, and libgit2 1.9.1 headers. A fifth parser/checked workload uses zlib's `adler32.c`, with five function bodies. Input manifests pin 151 physical files. Direct parser comparisons use the same frozen Clang 18 preprocessed source; checked and Builder comparisons exercise Toucan's preprocessor.

- Generated Rust is byte-identical for all four projects. Complete Builder reports are equal after excluding timings.
- All five direct parser ASTs have identical structure and values. Their source ranges now exclude trailing whitespace.
- All five checked compilations have identical semantic values, source, and dependencies. There are 17,133 whitespace trims and 19 explicitly reviewed ranges that restore an omitted closing `)` from an attribute. Each restoration is pinned to an exact input hash, JSON path, old/new range, and occurrence kind in [checked-span-corrections.json](checked-span-corrections.json). The comparator rejects other changes. Exact span and checked-JSON equality remain false.
- The existing 139 reference cases pass. Generated differential matrices compare 6,688 declaration cases and 1,136 expression cases with the old parser, with no acceptance or AST Printer differences. Printer comparisons do not compare spans or error wording.
- A separate 780-case statement/whitespace matrix found no AST shape or value differences. Its acceptance differences are variants of one corrected tokenization case: `0x1e+1` must be one invalid preprocessing number, rather than an addition. GCC and Clang confirm the rejection.
- Thirty additional tag-attribute cases now parse as declarations. The old direct parser rejected or misclassified them; the semantic source rewrite previously hid this defect. GCC and Clang accept all thirty.

The declaration matrix includes 720 native compiler checks and preserves existing parser/compiler differences, such as extension-profile restrictions and syntax accepted by the parser but rejected by semantic checking. The expression matrix separately checks all 549 intended-valid sources with GCC and Clang. These finite matrices do not establish complete C conformance. Full sources, expected outcomes, driver code, compiler diagnostics, and reproduction instructions are retained in `differential/` and the review archive.

## Native behavior

The final CLI passes 5,444 C/Rust constant and layout comparisons and real FFI calls against all four projects on x86-64 Linux. [validation/native-corpus.json](validation/native-corpus.json) lists the coverage. `validation/native-corpus.tar.xz` retains generated bindings, probes, compiler output, hashes, and every comparison.

The final default workspace run passes 1,136 tests, with 277 native/tool-dependent tests ignored by default. Clippy, formatting, and documentation with warnings denied pass. The earlier full native run found six failures across four targets; all four affected targets plus new source-range regressions pass in the final targeted native rerun (39 tests). The original failure log is preserved and is not represented as a clean full native run of the final sources. See [validation/checks.json](validation/checks.json).

The final c-testsuite acceptance gate also passes with both GCC and Clang preprocessing. All 211 cases accepted by both pedantic C11 compilers are accepted through both the preprocessed and native-source paths. The broader 220-case set remains at 219 accepted: `00144.c` discards a pointer qualifier, an existing semantic rejection also diagnosed by the pedantic compilers. See [validation/conformance.json](validation/conformance.json) for counts and compiler identities, and `validation/conformance.tar.xz` for every source, command, and diagnostic.

## Adversarial review

The [final independent review](adversarial-review.json) has no remaining actionable findings in its reviewed scope. It found and corrected three failures before the final capture:

1. The eager token buffer could allocate before enforcing its budget. Capacity is now checked before reservation, including unused slots.
2. Some nested expressions and statements could exhaust the fixed worker stack. A hard recursion ceiling of 512 now rejects those inputs with a resource error; C11 minimum nesting cases still pass. Successful AST clone/drop is also exercised on an ordinary caller stack.
3. The old semantic attribute rewrite changed byte positions without a complete source map. The parser now accepts attributes where they are written, allowing that rewrite to be removed. Regression tests assert the exact original operand and closing-delimiter spans.

Additional checks exercise UTF-8 byte boundaries in malformed C, literal prefixes, typedef shadowing, function-prototype scopes, longest-match tokens, all supported language/extension settings, exact work limits, and terminal resource exhaustion. The implementation forbids unsafe Rust.

The final AddressSanitizer campaign completed 1,028,421 executions in 901.11 seconds for a requested 900-second run, with no findings, no failure artifacts, and unchanged source hashes. A separate replay of 768 hostile nesting inputs also passed. The retained corpus grew from 2,939 to 7,161 inputs. Peak process RSS was 514 MiB under ASan; this includes sanitizer overhead and is not a production memory measurement.

[fuzz/summary.json](fuzz/summary.json) records the result and archive hash. `fuzz/parser-asan.tar.xz` includes the saved executable, source archive, initial/final corpora, hostile cases, and sanitizer controls. A deliberate heap-overflow control confirmed ASan detection. LeakSanitizer was disabled because its process-inspection probe failed in this environment. Earlier interrupted or infrastructure-failed attempts are retained separately and are not counted as completed campaigns.

## Performance

The table shows **less elapsed time**, using the median of seven paired process ratios for each workload. Every pair runs both variants, with five measured calls after a discarded warmup. All 196 processes and 980 timed calls passed their output checks. Every latency pair improved.

| Project | Binding generation | Checked compilation | Direct parsing |
| --- | ---: | ---: | ---: |
| zlib | 7.8% | 8.0% | 52.9% |
| SQLite | 32.8% | 22.7% | 71.1% |
| zstd | 14.5% | 16.8% | 59.4% |
| libgit2 | 16.9% | 10.9% | 51.8% |
| zlib `adler32.c` | — | 13.3% | 67.5% |

Separate allocation-counter builds passed 140 processes and 420 measured calls. Direct parsing makes 25–45% fewer allocation/reallocation calls and requests 12–30% fewer cumulative bytes. Binding generation makes 3–10% fewer calls and requests 1–7% fewer bytes. Those byte counts measure allocation traffic, not peak live heap. Whole-process median RSS decreased on all workloads, but includes output verification/serialization and is reported separately.

All timings come from the uninstrumented release binaries on one x86-64 Linux virtual machine, pinned to one CPU after local compilation, native probes, and fuzzing stopped. The paired ranges and raw samples are retained in the measurement evidence. These are warm repeated-call measurements, not cold process startup or tail-latency estimates.

## Reproduction

[METHODS.md](METHODS.md) describes the benchmark harness, build inputs, comparator, and replay commands. Differential and sanitizer archives include their own manifests and instructions. Baseline and candidate release builds use the same dependency versions, System allocator, fat LTO, and one codegen unit. The source/binary manifests identify the measured candidate independently of later test-only or documentation changes.

Measurements and native execution in this capture are Linux-only. They do not establish macOS or Windows performance, nor performance of a full optimizing C compiler. The added fuzz CI target runs on Linux; it does not add macOS Intel jobs.
