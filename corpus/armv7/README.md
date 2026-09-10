# ARMv7 GNU hard-float ABI acceptance

The branch-scoped `.github/workflows/armv7-native.yml` workflow installs the
ARMv7 GNU hard-float cross GCC toolchain, Clang, QEMU user mode, and the Rust
`armv7-unknown-linux-gnueabihf` standard library on an x86-64 Ubuntu runner.
It runs when `charlie/codex-toucan-armv7-native-acceptance` is pushed. The job
has a 20-minute limit and does not use macOS runners.

`layout.c` checks C sizes, alignments, offsets, and hard-float predefines with
GCC and Clang; both binaries run under QEMU. Toucan generates fresh bindings
from `abi.h` with its **Clang** target profile and a Rust 1.78 minimum. GCC is
an independent C ABI control, not a Toucan GCC ARM profile. The generated Rust
consumer calls C functions compiled by each compiler at `-O0` and `-O2`:
homogeneous float aggregates, a C-to-Rust callback, aligned records passed
by value, mixed register/stack parameters, packed fields, and bitfields.
The script checks ELF32 ARMv7 VFP object attributes and the linked hard-float
ELF flag before allowing QEMU execution.

```sh
python3 -B scripts/verify_armv7_native.py --toucan target/debug/toucan \
    --output /tmp/armv7-preflight --preflight
```

Preflight performs cross C and binding checks without installing the Rust ARM
standard library or cross GCC. Its evidence is marked `preflight-only`, with
`qemu_execution: false`. The full workflow saves
source, tool, binding, command, and binary hashes plus C/Rust results in
`results/armv7/native/evidence.json`. A green QEMU run establishes emulated
ARMv7 execution; native ARM hardware is outside this gate.

## Recorded result

At `8c8e74c`, [run 34385182838](https://github.com/astral-sh/toucan/actions/runs/34385182838)
passed two C layout executions and four C/Rust FFI executions under QEMU on
x86-64 Ubuntu. GCC and Clang each passed 256 FFI rounds at O0 and O2.
All twelve retained objects and executables have ARMv7/VFP signatures; all six
executables carry the hard-float ABI flag. Binding generation used no exclusions
and skipped no declarations or macros. The [preserved evidence](https://github.com/astral-sh/toucan/tree/27b1b56883b65c265b73630d9f674b504e28f776/corpus/evidence/armv7-qemu-2026-09-09/README.md)
records the exact source, commands, hashes, and independent archive checks.
This uses Toucan's Clang frontend profile and does not establish native ARM
hardware or full consumer acceptance.
