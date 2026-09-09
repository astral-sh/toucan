#!/usr/bin/env python3
"""Reproduce the conservative library-name guard from pinned compiler catalogs.

Supply the two upstream files locally. Hash checks reject changed inputs; this
extracts names only, including names disabled for some language/target profiles.
"""

from __future__ import annotations

import argparse
import hashlib
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "crates/toucan_semantic/src/implicit_library_names.rs"
INPUTS = {
    "clang": (
        (
            "https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-18.1.3/"
            "clang/include/clang/Basic/Builtins.def"
        ),
        "8b10e3289fee9e585d33ee61e12007a9a60a3c71890527a2d7235c27810a7002",
    ),
    "gcc": (
        (
            "https://raw.githubusercontent.com/gcc-mirror/gcc/releases/gcc-13.3.0/"
            "gcc/builtins.def"
        ),
        "bbd89bafc02c73b4ec6b15b4322fc517fa8fe58ded45a747892b0e4f12d78d0b",
    ),
}
GCC_MACROS = (
    "DEF_LIB_BUILTIN",
    "DEF_EXT_LIB_BUILTIN",
    "DEF_C94_BUILTIN",
    "DEF_C99_BUILTIN",
    "DEF_C11_BUILTIN",
    "DEF_C2X_BUILTIN",
    "DEF_C99_COMPL_BUILTIN",
    "DEF_C99_C90RES_BUILTIN",
    "DEF_EXT_C99RES_BUILTIN",
)


def pinned_input(path: Path, compiler: str) -> str:
    """Read exactly the catalog revision used by the modeled compiler profile."""
    contents = path.read_bytes()
    url, expected = INPUTS[compiler]
    if hashlib.sha256(contents).hexdigest() != expected:
        raise ValueError(f"unexpected {compiler} catalog; expected {url}")
    return contents.decode()


def generate(clang: str, gcc: str) -> str:
    """Extract ordinary library identifiers, including GCC's FloatN expansions."""
    names = set(re.findall(r"^LIBBUILTIN\(\s*(\w+)", clang, re.MULTILINE))
    macros = "|".join(GCC_MACROS)
    names.update(
        re.findall(rf'^(?:{macros})\s*\([^,]+,\s*"([^"]+)"', gcc, re.MULTILINE)
    )
    float_names = re.findall(
        r'^DEF_EXT_LIB_FLOATN_NX_BUILTINS\s*\([^,]+,\s*"([^"]+)"', gcc, re.MULTILINE
    )
    for name in float_names:
        names.update(
            name + suffix
            for suffix in ("f16", "f32", "f64", "f128", "f32x", "f64x", "f128x")
        )
    return (
        "// Conservative ordinary library-name union from LLVM 18.1.3 Builtins.def and GCC 13.3.0 builtins.def.\n"
        "// Names are facts; no compiler implementation or signature encoding is copied.\n"
        "const LIBRARY_NAMES: &[&str] = &[\n"
        + "".join(f'    "{name}",\n' for name in sorted(names))
        + "];\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--clang", type=Path, required=True)
    parser.add_argument("--gcc", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    result = generate(pinned_input(args.clang, "clang"), pinned_input(args.gcc, "gcc"))
    if args.check:
        if OUTPUT.read_text() != result:
            parser.error(f"{OUTPUT} differs from the pinned catalogs")
    else:
        OUTPUT.write_text(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
