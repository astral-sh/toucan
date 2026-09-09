# Native i686 zstd Rust consumer

At commit [`7b9f483`](https://github.com/astral-sh/toucan/commit/7b9f483ca2a9cafa45ca7829b762549f85fad8bf), [run 34378950445](https://github.com/astral-sh/toucan/actions/runs/34378950445) passed on Ubuntu with a native i686 glibc and Rust toolchain. [Artifact 10115177732](https://github.com/astral-sh/toucan/actions/runs/34378950445/artifacts/10115177732) is retained for 14 days. The compressed [evidence](evidence.json.gz) and [source hash](manifest.json) retain the outcome after artifact expiry.

The pinned `zstd-sys` build script, native 32-bit zstd C objects, and unchanged `zstd`/`zstd-safe` Rust wrappers built with upstream and Toucan-generated bindings. Only the two generated binding files changed. Rust dep-info confirms that the crate consumed those files. The two C archives have the same SHA-256; native ELF32 binaries ran four layout tests and round-tripped 58,000 bytes through bulk, streaming, and dictionary APIs. The four runtime artifacts (`bulk.zst`, `stream.zst`, `trained.zst`, and `trained.dict`) match byte for byte across generators. Native GCC and Clang checked the buffer layouts.

The check covers the default `zstd` feature set on i686 GNU Linux. It does not establish that the full uv/ty workspaces, all zstd configurations, or other 32-bit targets work.
