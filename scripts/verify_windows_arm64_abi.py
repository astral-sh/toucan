#!/usr/bin/env python3
"""Exercise generated bindings against native Windows ARM64 C and SDK oracles."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path

TARGET = "aarch64-pc-windows-msvc"

HEADER = """#if !defined(_M_ARM64) || !defined(_WIN64) || defined(_M_X64)
#error expected Windows ARM64
#endif

typedef struct Pair { long tag; double value; } Pair;
#pragma pack(push, 8)
typedef struct PackedPair { char first; double value; } PackedPair;
#pragma pack(pop)
typedef long (*Callback)(Pair);

#ifdef __clang__
typedef struct Natural128 { char first; __int128 second; unsigned __int128 third; } Natural128;
#pragma pack(push, 8)
typedef struct Packed128 { char first; __int128 second; } Packed128;
#pragma pack(pop)
#endif

#ifdef ARM64_PROBE_EXPORT
#define ARM64_PROBE_API __declspec(dllexport)
#else
#define ARM64_PROBE_API __declspec(dllimport)
#endif
ARM64_PROBE_API Pair call_pair(Pair input, Callback callback);
ARM64_PROBE_API int inspect_pair(Pair input);
"""

LAYOUT = """#include <stddef.h>
#include <windows.h>
#include "abi.h"
_Static_assert(sizeof(long) == 4 && sizeof(void *) == 8, "Windows LL64");
_Static_assert(sizeof(DWORD) == 4 && sizeof(SIZE_T) == 8 && sizeof(WCHAR) == 2,
               "Windows SDK scalar types");
_Static_assert(sizeof(Pair) == 16 && _Alignof(Pair) == 8 &&
               offsetof(Pair, value) == 8, "Pair layout");
_Static_assert(sizeof(PackedPair) == 16 && _Alignof(PackedPair) == 8 &&
               offsetof(PackedPair, value) == 8, "packed Pair layout");
#ifdef __clang__
_Static_assert(sizeof(Natural128) == 48 && _Alignof(Natural128) == 16 &&
               offsetof(Natural128, second) == 16 &&
               offsetof(Natural128, third) == 32, "natural int128 layout");
_Static_assert(sizeof(Packed128) == 24 && _Alignof(Packed128) == 8 &&
               offsetof(Packed128, second) == 8, "pack(8) int128 layout");
#endif
"""

C_LIBRARY = """#define ARM64_PROBE_EXPORT
#include "abi.h"
Pair call_pair(Pair input, Callback callback) {
    Pair result = { input.tag + callback(input), input.value + 3.25 };
    return result;
}
int inspect_pair(Pair input) { return input.tag == 16 && input.value == 5.75; }
"""

RUST_CONSUMER = """#![allow(dead_code, non_camel_case_types, non_snake_case, non_upper_case_globals)]
include!("bindings.rs");

unsafe extern "C" fn callback(input: Pair) -> ::core::ffi::c_long {
    input.tag + input.value as ::core::ffi::c_long
}

fn main() {
    assert_eq!((::core::mem::size_of::<Pair>(), ::core::mem::align_of::<Pair>()), (16, 8));
    assert_eq!(::core::mem::offset_of!(Pair, value), 8);
    assert_eq!((::core::mem::size_of::<Natural128>(), ::core::mem::align_of::<Natural128>()), (48, 16));
    assert_eq!(::core::mem::offset_of!(Natural128, second), 16);
    assert_eq!((::core::mem::size_of::<Packed128>(), ::core::mem::align_of::<Packed128>()), (24, 8));
    assert_eq!(::core::mem::offset_of!(Packed128, second), 8);
    let answer = unsafe { call_pair(Pair { tag: 7, value: 2.5 }, Some(callback)) };
    assert_eq!((answer.tag, answer.value), (16, 5.75));
    assert_eq!(unsafe { inspect_pair(answer) }, 1);
    println!("native Windows ARM64 C/Rust callback, record passing, and return passed");
}
"""


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_host(rustc_version: str) -> None:
    if sys.platform != "win32":
        raise RuntimeError("native Windows ARM64 host is required")
    if not re.search(r"^host: aarch64-pc-windows-msvc$", rustc_version, re.MULTILINE):
        raise RuntimeError("native aarch64-pc-windows-msvc Rust toolchain is required")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--clang-cl", default=os.environ.get("TOUCAN_CLANG_CL", "clang-cl.exe")
    )
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    evidence: dict = {
        "target": TARGET,
        "native_execution": False,
        "status": "failed",
        "architecture": platform.machine(),
        "commands": [],
        "checks": [],
    }

    def run(command: list[str | Path], name: str, timeout: int = 120) -> str:
        command = list(map(str, command))
        stdout = output / f"{name}.stdout"
        stderr = output / f"{name}.stderr"
        entry = {
            "name": name,
            "command": command,
            "stdout": stdout.name,
            "stderr": stderr.name,
        }
        evidence["commands"].append(entry)
        with stdout.open("w") as out, stderr.open("w") as err:
            result = subprocess.run(
                command,
                cwd=output,
                stdout=out,
                stderr=err,
                timeout=timeout,
                check=False,
            )
        entry["exit_code"] = result.returncode
        if result.returncode:
            raise RuntimeError(
                f"{name} failed: {stdout.read_text()[-4000:]} {stderr.read_text()[-4000:]}"
            )
        return stdout.read_text()

    try:
        # Python may report the process architecture when running under x64
        # emulation. The workflow checks the OS; rustc verifies the native host.
        if sys.platform != "win32":
            raise RuntimeError("native Windows ARM64 host is required")
        rustc_version = run(["rustc", "--version", "--verbose"], "rustc")
        check_host(rustc_version)
        evidence["rustc"] = rustc_version.strip()
        evidence["compiler_paths"] = {}
        for executable in ["cl.exe", args.clang_cl, "link.exe"]:
            path = shutil.which(executable)
            if not path:
                raise RuntimeError(f"MSVC/LLVM environment lacks {executable}")
            evidence["compiler_paths"][executable] = path
        evidence["windows_sdk_version"] = os.environ.get("WindowsSDKVersion")
        evidence["clang"] = run([args.clang_cl, "--version"], "clang-version").strip()
        toucan = args.toucan.resolve(strict=True)
        evidence["toucan_sha256"] = digest(toucan)

        for name, source in [
            ("abi.h", HEADER),
            ("layout.c", LAYOUT),
            ("library.c", C_LIBRARY),
            ("consumer.rs", RUST_CONSUMER),
        ]:
            (output / name).write_text(source, encoding="utf-8")
        msvc_version = run(
            [
                "cl.exe",
                "/Bv",
                "/std:c11",
                "/TC",
                "/c",
                "layout.c",
                "/Fo:msvc-layout.obj",
            ],
            "msvc-sdk-layout",
        )
        evidence["msvc_version"] = (
            msvc_version + (output / "msvc-sdk-layout.stderr").read_text()
        ).strip()
        evidence["checks"].append("msvc-windows-sdk-layout")
        run(
            [
                args.clang_cl,
                "--target=" + TARGET,
                "/nologo",
                "/std:c11",
                "/TC",
                "/c",
                "layout.c",
                "/Fo:clang-layout.obj",
            ],
            "clang-sdk-int128-layout",
        )
        evidence["checks"].append("clang-windows-sdk-and-int128-layout")
        run(
            [
                "cl.exe",
                "/nologo",
                "/std:c11",
                "/TC",
                "/LD",
                "library.c",
                "/Fe:arm64_probe.dll",
                "/link",
                "/MACHINE:ARM64",
            ],
            "build-msvc-dll",
        )
        run(
            [
                toucan,
                "bindgen",
                "abi.h",
                "--target",
                TARGET,
                "--dll-import-library",
                "*=arm64_probe",
                "--output",
                "bindings.rs",
            ],
            "generate-bindings",
        )
        bindings = output / "bindings.rs"
        source = bindings.read_text(encoding="utf-8")
        if not all(
            name in source
            for name in [
                "pub struct Pair",
                "pub struct Natural128",
                "pub struct Packed128",
                "pub fn call_pair(",
            ]
        ):
            raise RuntimeError("generated Rust is missing ABI declarations")
        if 'extern "win64"' in source or 'extern "sysv64"' in source:
            raise RuntimeError("unsupported ABI in Windows ARM64 bindings")
        evidence["bindings_sha256"] = digest(bindings)
        for level in ["0", "3"]:
            name = f"native-ffi-O{level}"
            run(
                [
                    "rustc",
                    "--edition=2021",
                    "-D",
                    "improper_ctypes",
                    "-C",
                    f"opt-level={level}",
                    "-L",
                    f"native={output}",
                    "consumer.rs",
                    "-o",
                    f"{name}.exe",
                ],
                f"compile-{name}",
            )
            run([output / f"{name}.exe"], f"run-{name}")
            evidence["checks"].append(name)
        if digest(toucan) != evidence["toucan_sha256"]:
            raise RuntimeError("Toucan executable changed during acceptance")
        evidence["native_execution"] = True
        evidence["status"] = "passed"
    except Exception as error:
        evidence["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(
            json.dumps(evidence, indent=2) + "\n", encoding="utf-8"
        )


if __name__ == "__main__":
    main()
