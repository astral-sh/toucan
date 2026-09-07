# Fuzzing

Install `cargo-fuzz` and a nightly Rust toolchain, then run a target from the repository root:

```console
cargo +nightly fuzz run preprocess -- -max_total_time=60
cargo +nightly fuzz run semantic -- -max_total_time=60
cargo +nightly fuzz run bindings -- -max_total_time=60
```

Targets exercise UTF-8 input, bounded preprocessing, declaration analysis, layout,
and binding generation. Invalid input may return a diagnostic; panics, aborts,
timeouts, and sanitizer failures are findings. Minimize failures and add regression
tests before fixing them. Short local runs are smoke tests, not a completed fuzz campaign.

The preprocessing targets disable filesystem access. Includes can resolve only to
the configured in-memory resource headers.

## Recorded smoke tests

The [local evidence](evidence.json) records 382,661 preprocessing inputs, 446,536
semantic inputs, and 25,192 binding inputs, with no findings in the final 61-second
runs. An earlier semantic run found a parser timeout; the declaration analyzer now
rejects the pathological prefix run and has a regression test for it.

AddressSanitizer was enabled. LeakSanitizer was disabled for these local runs because
it cannot inspect processes in the development environment's ptrace sandbox. CI
runs the default sanitizer configuration. These small runs leave substantial work
for sustained fuzzing and independent review.
