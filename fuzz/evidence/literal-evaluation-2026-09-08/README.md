# Literal evaluation binding campaign

The address-sanitized binding harness processed 34,955 inputs in 181.10 seconds with no findings, using eleven compiler/target profiles and GNU11/C11. Peak RSS was 555 MiB. LeakSanitizer was disabled because the environment uses ptrace.

The exact source manifest, initial corpus, dictionary, logs, and executable hash are retained. Source hashes were unchanged during the campaign. The frozen executable remains at `/home/dev-user/.cache/toucan/literal-evaluation-asan/bindings`. This bounded campaign does not replace sustained fuzzing.
