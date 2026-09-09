# ARMv7 GNU hard-float C/Rust execution under QEMU

At commit [`8c8e74c`](https://github.com/astral-sh/toucan/commit/8c8e74c7aa5598da20725b9247836a3559189d46), [run 34385182838](https://github.com/astral-sh/toucan/actions/runs/34385182838) passed on an Ubuntu 24.04 x86-64 runner. The ARMv7 binaries executed under QEMU's Cortex-A9 model with the GNU hard-float loader. This is emulated ARM execution, not a run on native ARM hardware. [Artifact 10117455703](https://github.com/astral-sh/toucan/actions/runs/34385182838/artifacts/10117455703) expires on September 23, 2026. The compressed [raw evidence](evidence.json.gz), [verification inputs](verification-inputs.json.gz), [summary](summary.json), and [manifest](manifest.json) preserve the recorded results.

The gate used Toucan's Clang profile for `armv7-unknown-linux-gnueabihf` and generated Rust with a minimum version of 1.78. The binding report contains eight declarations and five integer macros, with no skipped declarations, skipped macros, blocked functions, raw lines, or allowlist/blocklist arguments. Its sole header dependency is the unchanged `abi.h` fixture. The GNU frontend profile remains unsupported; GCC supplied an independent C ABI control.

| C compiler | C layout probe | C/Rust FFI at O0 | C/Rust FFI at O2 |
| --- | --- | --- | --- |
| GCC 13.3.0 | Passed | 256 rounds passed | 256 rounds passed |
| Clang 18.1.3 | Passed | 256 rounds passed | 256 rounds passed |

The C layout probes check 16 static assertions covering the data model, record and packed layouts, field offsets, bitfields, and constants. Each Rust executable calls C functions with floating-point aggregates, mixed records, stack arguments, packed records, and bitfields; C also calls back into Rust. Rust and C both use the listed optimization level. Rust 1.98.1 compiled the executables; QEMU 8.2.2 ran them.

Independent review verified all 52 successful command statuses and 104 stdout/stderr hashes. The four fixture files and harness script match the exact source commit. The generated binding hash matches the artifact, and all 12 retained object/executable hashes match the evidence. Reading those binaries independently confirms ELF32, ARM, EABI5, ARMv7 attributes, and VFP register arguments; all six executables additionally carry the hard-float ABI flag. The verification inputs retain source files, generated bindings, the binding report, commands' stdout/stderr, and the harness script.

The runner recorded hashes for Toucan, five tools, the ARM Rust standard library, and the GNU hard-float loader. Those binaries are absent from the uploaded artifact, so archive review could verify their recorded hashes and version-log consistency but could not independently rehash their bytes. The raw evidence preserves this provenance without claiming a reproducible toolchain build.

This verifies the checked-in ABI fixtures under emulation. Native ARM hardware, full uv/ty consumer builds, and sustained workload or performance validation remain outside this run.
