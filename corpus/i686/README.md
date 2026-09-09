# i686 GNU Linux native acceptance

The `i686-native.yml` workflow runs only when
`charlie/codex-toucan-i686-native-acceptance` is pushed. It uses one
`ubuntu-24.04` runner with a 15-minute job limit; it does not allocate macOS
runners. It installs GCC multilib, i686 glibc development files, Clang, and
the Rust `i686-unknown-linux-gnu` standard library.

`abi.h` is the exact header Toucan binds independently with its GCC and Clang
profiles. The header has no system includes; C and Rust see the same declarations.
`layout.c` independently asserts the i686 data model, double/long-double
alignment, aggregate sizes, offsets, packing, and constants using both native
32-bit C compilers. `abi.c` implements aggregate returns, a callback returning
an aggregate, stack parameters, packed values, and bitfields. The generated
`bindings.rs` is included by `abi.rs`, which calls those C functions in a
32-bit Rust process for 1,000 rounds per compiler and optimization level.

`scripts/verify_i686_native.py` checks ELF class **and** i386 machine for each
C layout executable, C ABI object, and Rust FFI executable before executing
it. It rejects missing or omitted bindings, compares the C/Rust results, and
compiles both archived Toucan bindings from the untouched pinned zstd 1.5.7
header as i686 Rust metadata. The zstd part checks Rust compilation and its
generated constant layout assertions; it does not call zstd C functions.

To reproduce on a Linux x86 host with GCC multilib, Clang, i686 glibc, and
Rust `i686-unknown-linux-gnu` std installed:

```sh
cargo build --locked -p toucan_cli
python3 scripts/verify_i686_native.py \
  --toucan target/debug/toucan --output results/i686/native
```

The always-uploaded `i686-native-<attempt>` artifact contains the fixture
sources, generated bindings, C/Rust binaries, hashes, versions, command
transcripts, install/build logs, and `native/evidence.json`. The latter says
`"status": "passed"` only after four 32-bit FFI executions and both zstd
compilations have completed. For machines without i686 glibc or Rust std,
`--preflight` runs only compiler syntax checks and binding generation, and
reports `"status": "preflight-only"` with `"native_execution": false`.

This probes ordinary i686 SysV C function calls. Nondefault `stdcall`,
`fastcall`, and `thiscall` conventions remain unsupported. Neither the
archived zstd Rust metadata nor the synthetic C library proves zstd runtime
compatibility or coverage of all glibc system headers.
