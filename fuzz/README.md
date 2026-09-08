# Fuzzing

Install `cargo-fuzz` and a nightly Rust toolchain, then run a target from the repository root:

```console
cargo +nightly fuzz run preprocess -- -max_total_time=60
cargo +nightly fuzz run semantic -- -max_total_time=60
cargo +nightly fuzz run bindings -- -max_total_time=60
cargo +nightly fuzz run checked -- -max_total_time=60
```

Targets exercise UTF-8 input, bounded preprocessing, declaration analysis, layout,
and binding generation. The `checked` target selects among all five target profiles
and compares analysis with and without retained code. Successful results must have
identical declarations; invalid inputs must produce the same diagnostic, except
when the separate retention limits are reached. It also exercises layout queries
on the retained analysis owner. This target explicitly rejects invalid UTF-8 so
the reproducer bytes are exactly the source used for analysis and target selection.
Invalid input may return a diagnostic; panics, aborts,
timeouts, and sanitizer failures are findings. Minimize failures and add regression
tests before fixing them. Short local runs are smoke tests, not a completed fuzz campaign.

The preprocessing targets disable filesystem access. Includes can resolve only to
the configured in-memory resource headers.

## Recorded smoke tests

The [local evidence](evidence.json) records 382,661 preprocessing inputs, 446,536
semantic inputs, and 25,192 binding inputs, with no findings in the final 61-second
runs. An earlier semantic run found a parser timeout; the declaration analyzer now
rejects the pathological prefix run and has a regression test for it.

A [later semantic smoke run](evidence/semantic-2026-09-08.json) processed 1,019,653
inputs in 181 seconds with no findings. Its report records the tested source and
binary hashes, seed, resource limits, and final libFuzzer statistics. The default
maximum mutation length was 4,096 bytes.

AddressSanitizer was enabled. LeakSanitizer was disabled for these local runs because
it cannot inspect processes in the development environment's ptrace sandbox. CI
runs the default sanitizer configuration. These small runs leave substantial work
for sustained fuzzing and independent review.

## Body checking and resource limits

The [body-checking campaign](evidence/readiness-2026-09-08.json) records three
301-second runs after the record traversal and parser-chain fixes:

| Target | Fuzzer inputs | Maximum input size | Peak RSS |
| --- | ---: | ---: | ---: |
| Preprocessor | 641,846 | 16 KiB | 525 MiB |
| Semantics and layout | 588,980 | 16 KiB | 513 MiB |
| Bindings | 50,858 | 8 KiB | 514 MiB |

All three runs completed without sanitizer findings, timeouts, or memory-limit
failures. The older typed `&str` harnesses can analyze a valid prefix before an invalid
UTF-8 byte; these counts do not measure accepted programs or output equivalence. The harnesses use the x86_64 Linux target
and the binding harness uses default options. New seeds cover GNU statement
expressions, variadic bodies, variable arrays, inline assembly, sparse
initializers, repeated record members, and control flow. The shared `c.dict`
provides C and preprocessor syntax for mutations. All semantic and binding seeds
were separately checked for frontend acceptance.

The accompanying boundedness review found two defects that mutation limits alone
would not reliably reach: repeated record-member traversal made a 981-byte
assignment exceed the five-second timeout, and 94–105 KiB label/statement chains
overflowed the parser stack. The report preserves reproducer hashes, before/after
results, compiler and binary hashes, source hashes, seeds, commands, and limits.
The fixes also passed 5,444 native corpus comparisons with unchanged Rust output.

AddressSanitizer was enabled. A direct LeakSanitizer probe failed because process
inspection is unavailable under ptrace; leak checking remained disabled. These
bounded local runs complement the compiler differential tests and native FFI
checks. They do not establish complete safety or frontend conformance.

To reproduce a target with the checked-in seeds and dictionary:

```console
mkdir -p /tmp/toucan-fuzz-semantic
cp fuzz/seeds/semantic/*.h /tmp/toucan-fuzz-semantic/
cargo +nightly fuzz run semantic /tmp/toucan-fuzz-semantic -- -dict=fuzz/c.dict -max_total_time=300 -max_len=16384 -len_control=0 -timeout=5 -rss_limit_mb=1024 -print_final_stats=1
```
