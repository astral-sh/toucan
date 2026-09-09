# Windows ARM64 native layout and FFI

The [GitHub run](https://github.com/astral-sh/toucan/actions/runs/34357566130)
passed at commit `4ce552f50909f86832cea5306fe07dff342a886b` in 2 minutes
58 seconds of job time. It used the standard `windows-11-vs2026-arm` runner,
native `aarch64-pc-windows-msvc` Rust 1.98.1, and Windows SDK 10.0.26100.0.
The [bounded workflow](../../../.github/workflows/windows-arm64.yml) has a
20-minute limit and triggers on exactly its acceptance branch; it has no
macOS job.

MSVC and LLVM independently compiled C11 assertions on Windows SDK scalar
types, `long`, and the checked C records. LLVM also confirmed 16-byte natural
alignment for `__int128` fields and 8-byte alignment with `#pragma pack(8)`.
Toucan generated fresh ARM64 Rust bindings for that C header. An MSVC-built
DLL accepted a Rust callback taking a struct by value and returned a struct
to Rust; both the unoptimized and optimized Rust callers passed. The runner
also passed the focused Toucan target, CLI, and frontend tests.

[artifact.zip](artifact.zip) is the exact GitHub Actions upload, including
generated Rust and C sources, compiler versions, command logs, native results,
and binaries. Its SHA-256 `72b0a6ac127f2cb4a88dae06b9fc216fbd0d510ce00ae92d765924be6901aff7`
matches the digest reported by GitHub for artifact `10106507656`. We checked
all 45 extracted files against the per-file hashes in
[summary.json](summary.json); the probe's generated-bindings hash and all ten
recorded command exit codes also agree. The archive is retained here after the
GitHub artifact's 30-day expiration.

The C layout probe includes `<windows.h>`, but the header passed to Toucan is
a small independent ABI fixture. This validates native ARM64 layout and FFI,
not Toucan parsing a full Windows SDK header or building Ruff/uv consumers on
Windows ARM64. It does not establish Mac Intel behavior or another target.
