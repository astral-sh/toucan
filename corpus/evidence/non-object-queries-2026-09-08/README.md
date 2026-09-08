# Void and function query validation

The copied candidate CLI matches **1,716 native compiler observations** with no
mismatches. `summary.json` records the source, archive, and binary hashes.

The main query matrix covers GCC 13 on x86-64/AArch64 and Clang 18 on all seven
physical targets, in C90, GNU90, C11, and GNU11 modes. Separate GNU musl probes
use the actual cached musl include trees for both architectures. Their
header-free query results match the corresponding GNU architecture rules.
Additional cases cover incomplete types, void indirection, function declaration
alignment, and aligned void/function typedefs. Each native row retains the C
source, exact compiler command, exit status, output, and diagnostics. Value rows
become static assertions in the candidate replay; rejection controls compare
acceptance rather than treating failed compilation as a value.

The archive also contains the checked-graph replay, test logs, compiler
identities, source manifest, and scripts. All **4,004** checked seed/profile/mode
pairs pass ordinary/retained equivalence and graph invariants. The semantic test
matrix checks all 11 profiles and four modes, with native GCC/Clang side-effect
runs. The C/Rust consumer fixture passes eight combinations per Rust version:
GCC/Clang, C optimization levels 0/2, and Rust levels 0/3. Both current Rust and
actual Rust 1.64 pass, including record-by-value pointer storage and callbacks in
both directions.

There is no new performance or sanitizer result in this layer. The parent
integration runs the full combined workspace and platform CI. Cross-target query
results are not native execution evidence; the native consumers recorded here
ran on x86-64 Linux. Scripts preserve the original devbox paths to compilers,
sysroots, and temporary C files. The recorded source and argv are sufficient to
recreate those invocations with equivalent toolchain paths.

The earlier unsupported aligned-typedef candidate and the comparator's initial
LLVM-output parsing failure remain in the external probe cache. Neither is used
as successful evidence in this archive. No production change was needed for the
comparator parsing correction.
