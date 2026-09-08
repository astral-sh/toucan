# Upstream import

- Project: [repr-c](https://github.com/mahkoh/repr-c).
- Author: Julian Orth <ju.orth@gmail.com>.
- Packages: `repc` 0.1.1 and `repc-impl` 0.1.1.
- Revision: `0c218ac5a6f82034e649fe749e7a902d7a43e8e0`, recorded by both published packages.
- Archive SHA-256: `8b9065c569206a9646c8cf6c6327fb29dbb2a0a89708958e0db964ffb256bb11`.
- Licenses: MIT OR Apache-2.0. License files and SPDX headers are copied unchanged.

This package combines the public facade and implementation in one crate. The
upstream target tables, ABI routing, layout types, visitor, and validation tests
are retained. The local semantic change adds an explicit compiler argument and
propagates it through recursive SysV/MinGW layouts. The original default entry
point delegates with the upstream system compiler. Only the two Clang-on-Linux
overrides have been enabled.

Other changes rename Rustdoc imports, format the imported source, keep internal
modules private, add documentation, and apply two equivalent modern Rust lint
fixes (`is_none` and `is_multiple_of`). The upstream `Into<()>` trait implementations
remain unchanged, with focused Clippy allowances to preserve that public API.
No source from `cly`, the reference-test generator, or other GPL tooling is included.

## Original file hashes

These SHA-256 values describe the published upstream source before adaptation.
Paths are relative to `repc-impl`, except the separately identified facade.

```json
{
  "build.rs": "a286168275a17c106d769ae1978a2ddf8d8372abc6d9d27caed5daadafd3cacd",
  "src/builder/common.rs": "5266d5a6bbee75a69226873cc4a06355a299290d5fe4b0cc64d82a8bd096d6c5",
  "src/builder/mod.rs": "0d7efa7b8d51c2873c93e9ff60f6041b84c702b5eecd8fb5e11680ce3a632e5d",
  "src/builder/msvc.rs": "271454534df329737fd69c6b5a787125a9e42a992afc1989ca88413030264739",
  "src/builder/sysv_like/mingw.rs": "d0fd65b5aa55beefc12bb3256ba12da220dd450fbbdc3264c1127e5253f13603",
  "src/builder/sysv_like/mod.rs": "d874e328cc70f826d7ea6ab570f1d877cff72d27d49be0cd47fb5560fe0161b6",
  "src/builder/sysv_like/sysv.rs": "89f5f2cf083bed88c5c2897e2a6f1bdde19c06bd4ad1d92c8bf5111ed5bf76c8",
  "src/layout.rs": "19b66bd5ebeb689c56c8c1cbc0e25a852dffa0a40c078278e329d38ec2a1e1cb",
  "src/lib.rs": "7a180ef570c78a0f4ab46a7db49b9bf88058a1564bbb6f13397d6ade5437b72d",
  "src/result.rs": "430f31a71ac0edd100fed1fa0aee2d411cb12a80462fc3edfcd5ac0925d33c53",
  "src/target.rs": "a5f513ada0b7f0df4989ab50f75c8ca1840f6ecfb34914fde95eff71336d118a",
  "src/tests.rs": "be404ef95c037c8101579245fe8d0f53c707f5874e23f6bb72c9f6a17b09ce77",
  "src/util.rs": "12476b43e943ba8eb1bd2976bbde40c07fa16ec7b4ce4d95bdf9ac00dffe2b5a",
  "src/visitor.rs": "5b683de909ff6c26b6ed081c4d50a2cd339039d2093610946fd839e1e8017d11",
  "repc/src/lib.rs": "665a8d79a6d65c95eff3a74e8a4f40cd7ac7bd7e916808ae686e05a3c3b46f10"
}
```
