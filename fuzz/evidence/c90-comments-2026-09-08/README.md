# C90 preprocessing policy campaign

The address-sanitized preprocessing harness processed 681,138 inputs in 181.14
seconds with no findings and 506 MiB peak RSS. This bounded local run verifies the
new selector; it does not replace sustained campaigns. LeakSanitizer was disabled
because the development environment runs under ptrace.

Selector version 2 keeps the original input bytes, query bit 0, and trigraph bit
0x100. `(sum(bytes) >> 9) % 5` selects enabled line comments, GCC C90 compilation,
GCC C90 preprocessing, Clang C90 compilation, or Clang C90 preprocessing. The
runner appends block-comment padding to every seed for all 20 combinations. The
starting archive contains 120 variants of six seeds and one invalid UTF-8 input.
The maximum padded seed is 395 bytes; mutations permit the full 16 KiB limit.

`selector-coverage.json` records each base seed hash and verifies its exact 20
variants in `initial-corpus.tar.gz`. `evidence.json` records commands, versions,
resource limits, binary and archive hashes, and source immutability. `source.json.gz`
contains the exact source manifest. `build.log.gz` and `fuzz.log.gz` retain raw logs.
The saved local executable is `/home/dev-user/.cache/toucan/c90-comment-fuzz-frozen/preprocess`.

The source is based on 1be727f plus the separately frozen comment-policy patch and
this harness change. It contains no in-progress C90 parser or semantic work.
The semantic, binding, and retained-code fuzz selector contracts are unchanged.
