# i686 GNU Linux header and layout probe

Source: untouched `zstd-1.5.7/lib/zstd.h` (SHA-256
`9b4bc8245565c98ccfc61c07749928b57e7c0f6fddb0530c4f6aa1971893d88b`),
and the adjacent `zstd_errors.h`, from the pinned Toucan corpus. The header only
requires `stddef.h` and `limits.h`, so this probe uses each compiler's own
cross-target headers. It does not use host x86-64 libc headers.
The `zstd_errors.h` SHA-256 is
`66a8c3f71d12ea6e797e4f622f31f3f8f81c41b36f48cad4f5de7d8bfb6aac0a`.

Clang 18.1.3 with `-target i686-unknown-linux-gnu` and GCC with `-m32` each
accepted the complete zstd header and all ten size, alignment, and field-offset
assertions in `zstd-layouts.c` under `-std=gnu11 -Werror -fsyntax-only`.

Toucan generated the two archived Rust files with `toucan bindgen zstd.h
--target i686-unknown-linux-gnu --compiler clang` and the corresponding `gcc`
profile. Each reported 86 declarations, no skipped declarations, and 15
skipped macros. Both outputs contain 191 public declarations and 278 lines.
They differ only in `wchar_t`: Clang defines it as `int` and GCC as `long int`
on this target. The native compiler macros and emitted `c_int` / `c_long`
types agree. Both outputs assert 12-byte size, 4-byte alignment, and field
offsets 0, 4, and 8 for both zstd buffer records.
The archived `zstd-clang.rs` and `zstd-gcc.rs` SHA-256 digests are
`bf7bf8d246345339860b51a141f5651bf30b53c7ae77b8178bcd4cbaed67b3b0`
and `81360c38c880a62fcc4289f05964df8bcd4253ef839cb4e7499fa981c365a330`.

To repeat the C probes, supply the pinned source directory as the include
path and compile `zstd-layouts.c` with each compiler:

```sh
clang -target i686-unknown-linux-gnu -std=gnu11 -Werror -fsyntax-only -I <zstd-1.5.7/lib> zstd-layouts.c
gcc -m32 -std=gnu11 -Werror -fsyntax-only -I <zstd-1.5.7/lib> zstd-layouts.c
```

To reproduce the binding outputs, run the built CLI on that same unmodified
header, writing each compiler profile's report beside the result:

```sh
toucan bindgen <zstd-1.5.7/lib/zstd.h> --target i686-unknown-linux-gnu --compiler clang --output zstd-clang.rs --report clang.json
toucan bindgen <zstd-1.5.7/lib/zstd.h> --target i686-unknown-linux-gnu --compiler gcc --output zstd-gcc.rs --report gcc.json
```

This proves cross-target compilation and selected layouts, not the complete
zstd ABI or running i686 FFI. There is no i686 glibc sysroot or installed Rust
i686 standard library in this test environment. glibc-dependent headers,
cross-built Rust bindings, and native FFI were not tested in this cross-target
probe. Later [native i686 evidence](../i686-native-af0d174-2026-09-09/README.md)
checks Rust metadata from these zstd bindings and 32-bit C/Rust calls on a
GitHub Ubuntu runner with the necessary development files.
Nondefault i686 `stdcall`, `fastcall`, and `thiscall` attributes fail
explicitly. Clang i686 accepts a 64-bit enum aligned to eight bytes whereas
its natural alignment is four; Toucan rejects that layout until the Rust
binding emitter can preserve the extra alignment.
