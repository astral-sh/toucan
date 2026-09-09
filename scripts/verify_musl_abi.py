#!/usr/bin/env python3
"""Run C/Rust ABI probes with explicit musl headers, compilers, and execution route."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import shlex
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGETS = ("x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(args: argparse.Namespace) -> dict:
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = ROOT / "corpus/musl"
    for name in ("abi.h", "abi.c", "abi.rs"):
        shutil.copyfile(source / name, output / name)
    report = {
        "status": "failed",
        "target": args.target,
        "compiler": args.compiler,
        "host": platform.platform(),
        "execution": "configured runner" if args.runner else "native",
        "runner": shlex.split(args.runner),
        "source_sha256": {
            name: digest(source / name) for name in ("abi.h", "abi.c", "abi.rs")
        },
        "toucan_sha256": digest(args.toucan),
        "commands": [],
        "rust_compilers": [],
        "runs": [],
    }

    def run(command: list[str | Path], name: str) -> str:
        command = [str(arg) for arg in command]
        entry = {"command": command, "name": name}
        report["commands"].append(entry)
        completed = subprocess.run(
            command,
            cwd=output,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        for stream in ("stdout", "stderr"):
            path = output / f"{name}.{stream}"
            path.write_text(getattr(completed, stream))
            entry[f"{stream}_sha256"] = digest(path)
        entry["exit_code"] = completed.returncode
        if completed.returncode:
            raise RuntimeError(
                f"{name} failed ({completed.returncode}): {completed.stderr[-4000:]}"
            )
        return completed.stdout

    try:
        architecture = args.target.split("-")[0]
        host = {"AMD64": "x86_64", "arm64": "aarch64"}.get(
            platform.machine(), platform.machine()
        )
        if not args.runner and (platform.system() != "Linux" or host != architecture):
            raise RuntimeError("foreign targets require an explicit --runner")
        cc = shlex.split(args.cc)
        report["c_compiler_version"] = run([*cc, "--version"], "c-version")
        report["c_compiler_sha256"] = digest(
            Path(shutil.which(cc[0]) or cc[0]).resolve()
        )
        linker = Path(shutil.which(args.rust_linker) or args.rust_linker).resolve()
        report["rust_linker_sha256"] = digest(linker)
        if args.runner:
            runner = shlex.split(args.runner)
            report["runner_version"] = run([*runner, "--version"], "runner-version")
            report["runner_sha256"] = digest(
                Path(shutil.which(runner[0]) or runner[0]).resolve()
            )
        include_args = []
        for path in args.include_dir:
            include_args.extend(["-I", path.resolve()])
        frontend = [
            args.toucan.resolve(),
            "--target",
            args.target,
            "--compiler",
            args.compiler,
            "--sysroot",
            args.sysroot.resolve(),
            *include_args,
        ]
        run(
            [
                frontend[0],
                "bindgen",
                output / "abi.h",
                *frontend[1:],
                "--allowlist",
                "Musl*",
                "--allowlist",
                "musl_*",
                "--allowlist",
                "MUSL_*",
                "--rust-target",
                "1.64",
                "--size-t-is-usize",
                "--report",
                "bindings-report.json",
                "-o",
                "bindings.rs",
            ],
            "bindings",
        )
        binding_report = json.loads((output / "bindings-report.json").read_text())
        dependencies = {
            path: digest(Path(path))
            for path in binding_report["dependencies"]
            if Path(path).is_file()
        }
        report["dependency_sha256"] = dependencies
        report["bindings_sha256"] = digest(output / "bindings.rs")
        for checked in (False, True):
            run(
                [
                    frontend[0],
                    "inspect",
                    output / "abi.c",
                    *frontend[1:],
                    *(["--checked-code"] if checked else []),
                    "-o",
                    f"analysis-{checked}.json",
                ],
                f"analysis-{checked}",
            )
        ordinary = json.loads((output / "analysis-False.json").read_text())
        retained = json.loads((output / "analysis-True.json").read_text())
        if ordinary["translation_unit"] != retained["translation_unit"]:
            raise RuntimeError("retaining checked code changed the translation unit")
        report["declarations_equal_with_retention"] = True
        for index, rustc in enumerate(args.rustc):
            rustc = Path(shutil.which(str(rustc)) or rustc).resolve()
            sysroot = Path(
                run([rustc, "--print", "sysroot"], f"rust-{index}-sysroot").strip()
            )
            libc = sysroot / "lib/rustlib" / args.target / "lib/self-contained/libc.a"
            report["rust_compilers"].append(
                {
                    "path": str(rustc),
                    "sha256": digest(rustc),
                    "version": run(
                        [rustc, "--version", "--verbose"], f"rust-{index}-version"
                    ),
                    "self_contained_libc_sha256": digest(libc),
                }
            )
        for copt in (0, 2):
            obj = output / f"abi-o{copt}.o"
            if args.assembler:
                assembly = output / f"abi-o{copt}.s"
                run(
                    [*cc, "-std=gnu11", f"-O{copt}", "-S", "abi.c", "-o", assembly],
                    f"c-o{copt}",
                )
                run(
                    [*shlex.split(args.assembler), "-c", assembly, "-o", obj],
                    f"assemble-o{copt}",
                )
            else:
                run(
                    [*cc, "-std=gnu11", f"-O{copt}", "-c", "abi.c", "-o", obj],
                    f"c-o{copt}",
                )
            for index, rustc in enumerate(args.rustc):
                for ropt in (0, 3):
                    name = f"abi-c{copt}-r{ropt}-compiler{index}"
                    binary = output / name
                    run(
                        [
                            rustc,
                            "--edition=2021",
                            "--target",
                            args.target,
                            "-C",
                            f"linker={linker}",
                            "-C",
                            f"link-arg={obj}",
                            "-C",
                            f"opt-level={ropt}",
                            "-D",
                            "improper_ctypes",
                            "-D",
                            "improper_ctypes_definitions",
                            "abi.rs",
                            "-o",
                            binary,
                        ],
                        name,
                    )
                    result = run([*shlex.split(args.runner), binary], f"execute-{name}")
                    if (
                        result
                        != "musl ABI: 49 dimensions; 1000 aggregate, callback, stack, variadic, bitfield, atomic rounds\n"
                    ):
                        raise RuntimeError(f"unexpected probe result: {result!r}")
                    report["runs"].append(
                        {
                            "c_optimization": copt,
                            "rust_optimization": ropt,
                            "rust_compiler": index,
                            "result": result,
                            "binary_sha256": digest(binary),
                        }
                    )
        if dependencies != {path: digest(Path(path)) for path in dependencies}:
            raise RuntimeError("an input header changed during validation")
        if digest(args.toucan) != report["toucan_sha256"]:
            raise RuntimeError("the frontend executable changed during validation")
        report["status"] = "passed"
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--compiler", choices=("gcc", "clang"), required=True)
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--include-dir", type=Path, action="append", default=[])
    parser.add_argument(
        "--cc", required=True, help="C compiler and flags, parsed with shell quoting"
    )
    parser.add_argument(
        "--assembler", default="", help="Optional separate assembler and flags"
    )
    parser.add_argument("--rust-linker", required=True)
    parser.add_argument("--rustc", type=Path, action="append", required=True)
    parser.add_argument(
        "--runner", default="", help="Explicit runner and flags for a foreign CPU"
    )
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument(
        "--output", type=Path, required=True, help="Fresh output directory"
    )
    report = verify(parser.parse_args())
    print(
        f"{report['target']} {report['compiler']}: {len(report['runs'])} ABI executions passed ({report['execution']})"
    )


if __name__ == "__main__":
    main()
