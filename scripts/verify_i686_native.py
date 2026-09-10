#!/usr/bin/env python3
"""Check the i686 GNU C ABI against generated bindings in a native 32-bit process."""

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
TARGET = "i686-unknown-linux-gnu"
EXPECTED = (
    "i686 C/Rust FFI: 1000 aggregate, callback, stack, and bitfield rounds passed\n"
)
SOURCES = ("abi.h", "abi.c", "abi.rs", "layout.c")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(output: Path, toucan: Path, preflight: bool) -> dict:
    output = output.resolve()
    toucan = toucan.resolve()
    output.mkdir(parents=True, exist_ok=False)
    sources = ROOT / "corpus/i686"
    report = {
        "status": "failed",
        "target": TARGET,
        "host": platform.platform(),
        "python": sys.version,
        "mode": "cross-target syntax and binding generation only" if preflight else "native 32-bit execution",
        "native_execution": False,
        "source_sha256": {name: sha256(sources / name) for name in SOURCES},
        "script_sha256": sha256(Path(__file__)),
        "archived_zstd_sha256": {
            compiler: sha256(ROOT / f"corpus/i686/zstd-{compiler}.rs")
            for compiler in ("gcc", "clang")
        },
        "commands": [],
        "runs": [],
    }

    def run(command: list[str | Path], name: str, cwd: Path = output) -> str:
        command = [str(arg) for arg in command]
        entry = {"name": name, "argv": command, "cwd": str(cwd)}
        report["commands"].append(entry)
        completed = subprocess.run(
            command, cwd=cwd, capture_output=True, text=True, timeout=120, check=False
        )
        for stream in ("stdout", "stderr"):
            path = output / f"{name}.{stream}"
            path.write_text(getattr(completed, stream))
            entry[f"{stream}_sha256"] = sha256(path)
        entry["exit_code"] = completed.returncode
        if completed.returncode:
            raise RuntimeError(f"{name}: exit {completed.returncode}: {completed.stderr[-3000:]}")
        return completed.stdout

    def require_i386(path: Path, name: str) -> None:
        header = run(["readelf", "--wide", "--file-header", path], f"elf-{name}")
        if not re.search(r"^\s*Class:\s+ELF32\s*$", header, re.MULTILINE) or not re.search(
            r"^\s*Machine:\s+Intel 80386\s*$", header, re.MULTILINE
        ):
            raise RuntimeError(f"{path} is not ELF32 i386; refusing to accept its execution")
        report["runs"].append({"kind": "elf32-i386", "path": str(path), "sha256": sha256(path)})

    try:
        if platform.system() != "Linux" or platform.machine() not in ("x86_64", "i686"):
            raise RuntimeError("i686 acceptance requires an x86 Linux host")
        if not toucan.is_file():
            raise RuntimeError(f"missing host-built Toucan CLI: {toucan}")
        report["toucan_sha256"] = sha256(toucan)
        for tool, args in (
            ("gcc", ["--version"]),
            ("clang", ["--version"]),
            ("readelf", ["--version"]),
            ("rustc", ["--version", "--verbose"]),
            ("cargo", ["--version"]),
        ):
            binary = (
                Path(run(["rustup", "which", tool], f"path-{tool}").strip())
                if tool in ("rustc", "cargo")
                else Path(shutil.which(tool) or tool)
            ).resolve()
            report.setdefault("tool_sha256", {})[tool] = sha256(binary)
            report.setdefault("tool_versions", {})[tool] = run([tool, *args], f"version-{tool}")
        if not preflight:
            sysroot = Path(run(["rustc", "--print", "sysroot"], "rust-sysroot").strip())
            target_lib = sysroot / "lib/rustlib" / TARGET / "lib"
            stdlib = list(target_lib.glob("libstd-*.rlib"))
            if len(stdlib) != 1:
                raise RuntimeError(f"expected exactly one installed {TARGET} Rust std: {stdlib}")
            report["rust_std_sha256"] = {str(stdlib[0]): sha256(stdlib[0])}
        for compiler in ("gcc", "clang"):
            directory = output / compiler
            directory.mkdir()
            for name in SOURCES:
                shutil.copyfile(sources / name, directory / name)
            cc = ["gcc", "-m32"] if compiler == "gcc" else ["clang", f"--target={TARGET}", "-m32"]
            run([*cc, "-std=gnu11", "-Werror", "-fsyntax-only", "layout.c", "abi.c"], f"{compiler}-syntax", directory)
            run(
                [
                    toucan,
                    "bindgen",
                    "abi.h",
                    "--target",
                    TARGET,
                    "--compiler",
                    compiler,
                    "--report",
                    "binding-report.json",
                    "--output",
                    "bindings.rs",
                ],
                f"{compiler}-bindings",
                directory,
            )
            bindings = json.loads((directory / "binding-report.json").read_text())
            if bindings["target"] != TARGET or bindings["compiler"] != compiler:
                raise RuntimeError(f"{compiler}: binding report selected the wrong target/profile")
            if bindings["skipped_declarations"] or bindings["blocked_functions"] or bindings["skipped_macros"]:
                raise RuntimeError(f"{compiler}: omitted bindings: {bindings}")
            dependencies = {str(Path(path).resolve()): sha256(Path(path)) for path in bindings["dependencies"]}
            if dependencies != {str((directory / "abi.h").resolve()): sha256(directory / "abi.h")}:
                raise RuntimeError(f"{compiler}: bindings read unexpected headers: {dependencies}")
            report.setdefault("binding_sha256", {})[compiler] = sha256(directory / "bindings.rs")
            report.setdefault("binding_dependencies", {})[compiler] = dependencies
            if preflight:
                continue

            layout = directory / "layout"
            run([*cc, "-std=gnu11", "-Werror", "layout.c", "-o", layout], f"{compiler}-layout-build", directory)
            require_i386(layout, f"{compiler}-layout")
            if run([layout], f"{compiler}-layout-run", directory) != "i686 C layout: passed\n":
                raise RuntimeError(f"{compiler}: unexpected native C layout probe output")

            for opt in (0, 2):
                obj = directory / f"abi-o{opt}.o"
                # rustc links with -nodefaultlibs, which does not provide the
                # i386 GCC stack-protector helper for a separately built C object.
                run([*cc, "-std=gnu11", "-Werror", "-fno-stack-protector", f"-O{opt}", "-c", "abi.c", "-o", obj], f"{compiler}-c-o{opt}", directory)
                require_i386(obj, f"{compiler}-c-o{opt}")
                binary = directory / f"ffi-o{opt}"
                run(
                    [
                        "rustc",
                        "--edition=2021",
                        "--target",
                        TARGET,
                        "-C",
                        "linker=gcc",
                        "-C",
                        "link-arg=-m32",
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
                    f"{compiler}-rust-o{opt}",
                    directory,
                )
                require_i386(binary, f"{compiler}-rust-o{opt}")
                if run([binary], f"{compiler}-ffi-o{opt}", directory) != EXPECTED:
                    raise RuntimeError(f"{compiler}: unexpected native FFI output (O{opt})")
                report["runs"].append({"kind": "native-ffi", "compiler": compiler, "optimization": opt, "binary_sha256": sha256(binary), "rounds": 1000})

            archived = ROOT / f"corpus/i686/zstd-{compiler}.rs"
            metadata = directory / f"zstd-{compiler}.rmeta"
            run(
                [
                    "rustc",
                    "--edition=2021",
                    "--target",
                    TARGET,
                    "--crate-name",
                    f"zstd_{compiler}",
                    "--crate-type=lib",
                    "--emit=metadata",
                    "-A",
                    "warnings",
                    archived,
                    "-o",
                    metadata,
                ],
                f"{compiler}-zstd-rust",
                directory,
            )
            report.setdefault("zstd_metadata_sha256", {})[compiler] = sha256(metadata)

        if sha256(toucan) != report["toucan_sha256"] or any(
            sha256(sources / name) != digest for name, digest in report["source_sha256"].items()
        ):
            raise RuntimeError("Toucan or the source files changed while running the probes")
        report["native_execution"] = not preflight
        report["status"] = "preflight-only" if preflight else "passed"
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toucan", type=Path, required=True, help="Host-built Toucan binary")
    parser.add_argument("--output", type=Path, required=True, help="Fresh artifact subdirectory")
    parser.add_argument("--preflight", action="store_true", help="Only generate bindings and compile C syntax; does not establish native acceptance")
    args = parser.parse_args()
    report = verify(args.output, args.toucan, args.preflight)
    print(f"i686 GNU acceptance: {report['status']}; native FFI executions: {sum(run['kind'] == 'native-ffi' for run in report['runs'])}")


if __name__ == "__main__":
    main()
