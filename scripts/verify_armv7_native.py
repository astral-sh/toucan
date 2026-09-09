#!/usr/bin/env python3
"""Check the ARMv7 GNU hard-float C ABI against Toucan bindings under QEMU."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGET = "armv7-unknown-linux-gnueabihf"
GCC = "arm-linux-gnueabihf-gcc"
QEMU = "qemu-arm"
SYSROOT = Path("/usr/arm-linux-gnueabihf")
SOURCES = ("abi.h", "abi.c", "abi.rs", "layout.c")
CFLAGS = (
    "-march=armv7-a",
    "-mfpu=vfpv3-d16",
    "-mfloat-abi=hard",
    "-std=gnu11",
    "-Werror",
)
EXPECTED_LAYOUT = "ARMv7 hard-float C layout: passed\n"
EXPECTED_FFI = "ARMv7 hard-float C/Rust FFI: 256 aggregate, callback, stack, and bitfield rounds passed\n"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def arm_elf_error(header: str, attributes: str, *, executable: bool) -> str | None:
    """Reject soft-float, wrong-architecture, or non-EABI compiler outputs."""
    required = (
        (r"^\s*Class:\s+ELF32\s*$", "ELF32"),
        (r"^\s*Machine:\s+ARM\s*$", "ARM machine"),
        (r"^\s*Flags:.*Version5 EABI", "EABI5"),
    )
    for pattern, name in required:
        if not re.search(pattern, header, re.MULTILINE):
            return f"missing {name}"
    for pattern, name in (
        (r"^\s*Tag_CPU_arch:\s+v7\s*$", "ARMv7"),
        (r"^\s*Tag_ABI_VFP_args:\s+VFP registers\s*$", "VFP register arguments"),
    ):
        if not re.search(pattern, attributes, re.MULTILINE):
            return f"missing {name}"
    if executable and not re.search(
        r"^\s*Flags:.*hard-float ABI", header, re.MULTILINE
    ):
        return "missing hard-float executable ABI"
    return None


def verify(output: Path, toucan: Path, clang: str, preflight: bool) -> dict:
    output = output.resolve()
    toucan = toucan.resolve()
    output.mkdir(parents=True, exist_ok=False)
    sources = ROOT / "corpus/armv7"
    report = {
        "status": "failed",
        "target": TARGET,
        "host": platform.platform(),
        "python": sys.version,
        "mode": "cross C syntax and binding generation only"
        if preflight
        else "QEMU ARMv7 execution",
        "qemu_execution": False,
        "native_hardware_execution": False,
        "source_sha256": {name: sha256(sources / name) for name in SOURCES},
        "script_sha256": sha256(Path(__file__)),
        "commands": [],
        "runs": [],
    }

    def run(command: list[str | Path], name: str) -> str:
        argv = [str(arg) for arg in command]
        entry = {"name": name, "argv": argv, "cwd": str(output)}
        report["commands"].append(entry)
        completed = subprocess.run(
            argv, cwd=output, capture_output=True, text=True, timeout=120, check=False
        )
        for stream in ("stdout", "stderr"):
            path = output / f"{name}.{stream}"
            path.write_text(getattr(completed, stream))
            entry[f"{stream}_sha256"] = sha256(path)
        entry["exit_code"] = completed.returncode
        if completed.returncode:
            raise RuntimeError(
                f"{name}: exit {completed.returncode}: {completed.stderr[-3000:]}"
            )
        return completed.stdout

    def require_armv7(path: Path, name: str, *, executable: bool = False) -> None:
        header = run(["readelf", "--wide", "--file-header", path], f"elf-header-{name}")
        attributes = run(["readelf", "--arch-specific", path], f"elf-attributes-{name}")
        error = arm_elf_error(header, attributes, executable=executable)
        if error:
            raise RuntimeError(f"{path}: {error}; refusing to accept its execution")
        report["runs"].append(
            {"kind": "armv7-elf", "path": str(path), "sha256": sha256(path)}
        )

    try:
        if platform.system() != "Linux" or platform.machine() != "x86_64":
            raise RuntimeError("ARMv7 QEMU acceptance requires an x86_64 Linux runner")
        if not toucan.is_file():
            raise RuntimeError(f"missing host-built Toucan CLI: {toucan}")
        report["toucan_sha256"] = sha256(toucan)
        tools = {"clang": clang, "readelf": "readelf", "rustc": "rustc"}
        if not preflight:
            tools.update({"gcc": GCC, "qemu": QEMU})
        for name, tool in tools.items():
            binary = shutil.which(tool)
            if binary is None:
                raise RuntimeError(f"missing required tool: {tool}")
            report.setdefault("tool_sha256", {})[name] = sha256(Path(binary).resolve())
            args = ["--version", "--verbose"] if name == "rustc" else ["--version"]
            report.setdefault("tool_versions", {})[name] = run(
                [tool, *args], f"version-{name}"
            )
        rust_version = report["tool_versions"]["rustc"]
        match = re.search(r"^rustc 1\.(\d+)\.", rust_version)
        if not match or int(match.group(1)) < 78:
            raise RuntimeError("ARMv7 bindings require stable Rust 1.78 or newer")
        rust_cfg = run(
            ["rustc", "--print", "cfg", "--target", TARGET], "rust-target-cfg"
        )
        for key in (
            'target_arch="arm"',
            'target_os="linux"',
            'target_env="gnu"',
            'target_abi="eabihf"',
        ):
            if key not in rust_cfg.splitlines():
                raise RuntimeError(f"Rust target {TARGET} lacks {key}")
        if not preflight:
            stdlib = list(
                (
                    Path(run(["rustc", "--print", "sysroot"], "rust-sysroot").strip())
                    / "lib/rustlib"
                    / TARGET
                    / "lib"
                ).glob("libstd-*.rlib")
            )
            if len(stdlib) != 1:
                raise RuntimeError(
                    f"expected exactly one installed {TARGET} Rust std: {stdlib}"
                )
            report["rust_std_sha256"] = {str(stdlib[0]): sha256(stdlib[0])}
            loader = SYSROOT / "lib/ld-linux-armhf.so.3"
            if not loader.is_file():
                raise RuntimeError(f"missing GNU hard-float ARM loader: {loader}")
            report["loader_sha256"] = sha256(loader)

        for name in SOURCES:
            shutil.copyfile(sources / name, output / name)
        compiler = [clang, f"--target={TARGET}", *CFLAGS]
        predefined = run(
            [*compiler, "-dM", "-E", "-x", "c", "/dev/null"], "clang-predefined"
        )
        for macro in (
            "__ARM_PCS_VFP 1",
            "__ARM_ARCH_7A__ 1",
            "__CHAR_UNSIGNED__ 1",
            "__WCHAR_UNSIGNED__ 1",
            "__SIZEOF_LONG_DOUBLE__ 8",
        ):
            if f"#define {macro}\n" not in predefined:
                raise RuntimeError(
                    f"Clang {TARGET} lacks required hard-float C macro: {macro}"
                )
        run([*compiler, "-fsyntax-only", "layout.c", "abi.c"], "clang-c-syntax")
        clang_layout = output / "clang-layout.o"
        run(
            [*compiler, "-fno-stack-protector", "-c", "layout.c", "-o", clang_layout],
            "clang-layout-object",
        )
        require_armv7(clang_layout, "clang-layout")
        run(
            [
                toucan,
                "bindgen",
                "abi.h",
                "--target",
                TARGET,
                "--compiler",
                "clang",
                "--rust-target",
                "1.78",
                "--report",
                "binding-report.json",
                "--output",
                "bindings.rs",
            ],
            "toucan-bindings",
        )
        bindings = json.loads((output / "binding-report.json").read_text())
        if (
            bindings["target"] != TARGET
            or bindings["compiler"] != "clang"
            or bindings["rust_target"] != "1.78"
            or bindings["skipped_declarations"]
            or bindings["blocked_functions"]
            or bindings["skipped_macros"]
        ):
            raise RuntimeError(
                f"Toucan selected the wrong ABI or omitted declarations: {bindings}"
            )
        deps = {
            str(Path(path).resolve()): sha256(Path(path))
            for path in bindings["dependencies"]
        }
        if deps != {str((output / "abi.h").resolve()): sha256(output / "abi.h")}:
            raise RuntimeError(f"Toucan read unexpected headers: {deps}")
        report["binding_sha256"] = sha256(output / "bindings.rs")
        report["binding_dependencies"] = deps
        generated = (output / "bindings.rs").read_text()
        if (
            'target_abi = "eabihf"' not in generated
            or "pub fn armv7_callback(" not in generated
            or "pub fn armv7_record(" not in generated
            or "pub fn armv7_stack(" not in generated
        ):
            raise RuntimeError(
                "bindings lack the exact ARMv7 guard or required C calls"
            )

        if not preflight:
            gcc = [GCC, *CFLAGS, "-fno-stack-protector"]
            for kind, cc in (("gcc", gcc), ("clang", compiler)):
                if kind == "gcc":
                    layout_object = output / "gcc-layout.o"
                    run(
                        [*cc, "-c", "layout.c", "-o", layout_object],
                        "gcc-layout-object",
                    )
                    require_armv7(layout_object, "gcc-layout")
                else:
                    layout_object = clang_layout
                layout = output / f"{kind}-layout"
                run([*gcc, layout_object, "-o", layout], f"{kind}-layout-link")
                require_armv7(layout, f"{kind}-layout-executable", executable=True)
                if (
                    run(
                        [QEMU, "-cpu", "cortex-a9", "-L", SYSROOT, layout],
                        f"{kind}-layout-run",
                    )
                    != EXPECTED_LAYOUT
                ):
                    raise RuntimeError(f"{kind}: unexpected QEMU C layout probe output")
                report["runs"].append({"kind": "qemu-layout", "compiler": kind})
                for opt in (0, 2):
                    obj = output / f"{kind}-abi-o{opt}.o"
                    run(
                        [
                            *cc,
                            "-fno-stack-protector",
                            f"-O{opt}",
                            "-c",
                            "abi.c",
                            "-o",
                            obj,
                        ],
                        f"{kind}-abi-object-o{opt}",
                    )
                    require_armv7(obj, f"{kind}-abi-o{opt}")
                    binary = output / f"{kind}-ffi-o{opt}"
                    run(
                        [
                            "rustc",
                            "--edition=2021",
                            "--target",
                            TARGET,
                            "-C",
                            f"linker={GCC}",
                            "-C",
                            f"link-arg={obj}",
                            "-C",
                            f"opt-level={opt}",
                            "-D",
                            "improper_ctypes",
                            "-D",
                            "improper_ctypes_definitions",
                            "abi.rs",
                            "-o",
                            binary,
                        ],
                        f"{kind}-rust-o{opt}",
                    )
                    require_armv7(binary, f"{kind}-rust-o{opt}", executable=True)
                    if (
                        run(
                            [QEMU, "-cpu", "cortex-a9", "-L", SYSROOT, binary],
                            f"{kind}-ffi-o{opt}",
                        )
                        != EXPECTED_FFI
                    ):
                        raise RuntimeError(
                            f"{kind} O{opt}: unexpected C/Rust FFI output"
                        )
                    report["runs"].append(
                        {
                            "kind": "qemu-ffi",
                            "compiler": kind,
                            "optimization": opt,
                            "rounds": 256,
                            "binary_sha256": sha256(binary),
                        }
                    )

        if sha256(toucan) != report["toucan_sha256"] or any(
            sha256(sources / name) != digest
            for name, digest in report["source_sha256"].items()
        ):
            raise RuntimeError(
                "Toucan or the C/Rust source files changed during acceptance"
            )
        report["qemu_execution"] = not preflight
        report["status"] = "preflight-only" if preflight else "passed"
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--toucan", type=Path, required=True, help="Host-built Toucan CLI"
    )
    parser.add_argument(
        "--output", type=Path, required=True, help="Fresh evidence directory"
    )
    parser.add_argument("--clang", default="clang", help="Clang with the ARMv7 backend")
    parser.add_argument(
        "--preflight",
        action="store_true",
        help="Cross C and binding checks only; no FFI acceptance",
    )
    args = parser.parse_args()
    report = verify(args.output, args.toucan, args.clang, args.preflight)
    count = sum(run["kind"] == "qemu-ffi" for run in report["runs"])
    print(
        f"ARMv7 GNU hard-float acceptance: {report['status']}; QEMU C/Rust FFI executions: {count}"
    )


if __name__ == "__main__":
    main()
