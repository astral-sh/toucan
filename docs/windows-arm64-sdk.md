# Native Windows ARM64 SDK gate

The `Windows ARM64 SDK headers` workflow runs only on pushes to
`charlie/codex-toucan-windows-arm64-sdk`. Its job also checks the exact branch
before allocating `windows-11-vs2026-arm`. It has a 12-minute limit and no pull
request, default-branch, manual-dispatch, or macOS trigger.

The job selects native ARM64 Rust, invokes the installed Visual Studio
`vcvarsall.bat arm64`, and builds the CLI with two Cargo jobs. The script takes
the actual SDK version and include search order from that environment. It does
not download substitute headers, edit the installed SDK, or erase unsupported
declarations.

## Required cases

| Input | Configuration | Checked Rust types |
| --- | --- | --- |
| `basetsd.h` | Direct include | Six signed/unsigned pointer-sized aliases |
| `winnt.h` | `excpt.h`, `minwindef.h`, and `_ARM64_` prerequisites | `DWORD`, `WCHAR`, `LARGE_INTEGER`, `ULARGE_INTEGER` |
| `windows.h` | `WIN32_LEAN_AND_MEAN=1` | Scalar types, `FILETIME`, `SYSTEMTIME`, `POINT`, `RECT` |

The low-level `winnt.h` case uses the architecture definition and prerequisites
normally supplied through `windows.h`. Microsoft's published
[Windows header](https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/um/Windows.h)
selects `_ARM64_`, and
[minwindef.h](https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/shared/minwindef.h)
supplies the scalar types before including `winnt.h`. Those sources informed the
wrapper; the job tests the installed SDK files and records their actual hashes.

Each complete header input goes through MSVC and Clang-cl C11 compilation,
Toucan preprocessing and declaration/body checking, and binding generation for
the listed types. Native Rust then checks the same sizes and alignments as the
C `_Static_assert` controls, plus five field offsets for `windows.h`. The C
objects and Rust executables must identify themselves as ARM64 COFF/PE.

Toucan explicitly selects its Windows Clang C11 profile. The installed MSVC and
Clang versions can differ from that modeled profile. The artifact retains all
three sets of relevant predefined macros; this check does not assume identical
preprocessing branches or claim an MSVC compiler-version profile.

## Results and limits

All three cases are required. Any C-oracle rejection, Toucan diagnostic, missing
SDK dependency, generated-Rust failure, incorrect layout, or timeout leaves the
job failed. Later independent cases can still record results within the
330-second script deadline; each command has a 45-second limit. Failed and
unstarted phases remain explicit in `evidence.json`, and artifact upload runs
after failures.

Artifacts contain SDK/tool paths and versions, include order, wrapper sources,
commands with exit codes, diagnostics, preprocessed C, generated Rust and its
report, native dependency hashes, and the Rust execution results. Physical
header inputs must remain unchanged. The generator has a two-million-token
preprocessing limit; hitting it is a failed bounded check, not acceptance.

The [native run](https://github.com/astral-sh/toucan/actions/runs/34378135474)
passed at `8ec8ad7` with installed SDK `10.0.26100.0`. The
[preserved evidence](../corpus/evidence/windows-arm64-sdk-2026-09-09/README.md)
records all seven successful stages for each header, unchanged SDK inputs, and
the hashes of the tested tools and dependencies. This proves the listed header
configurations and selected layouts for that SDK and target. It does not cover
all Windows APIs, C++, other SDK versions, or general Windows FFI calls.
