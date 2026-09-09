#!/usr/bin/env python3
"""Check installed Windows SDK C headers and generated Rust on native Windows."""

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
import time
from pathlib import Path

TARGET = "aarch64-pc-windows-msvc"
TARGETS = {
    TARGET: {
        "vcvars": "arm64",
        "machine": 0xAA64,
        "define": "_ARM64_",
        "guard": "!defined(_M_ARM64) || !defined(_WIN64) || defined(_M_X64)",
    },
    "x86_64-pc-windows-msvc": {
        "vcvars": "x64",
        "machine": 0x8664,
        "define": "_AMD64_",
        "guard": "!defined(_M_X64) || !defined(_WIN64) || defined(_M_ARM64)",
    },
}
RUN_SECONDS = 330
COMMAND_SECONDS = 45
REQUIRED_STAGES = ("msvc", "clang", "preprocess", "check", "bindgen", "rustc", "execute")
CASES = {
    "basetsd": {
        "header": "shared/basetsd.h",
        "includes": "#include <basetsd.h>\n",
        "layouts": [(name, 8, 8) for name in ("INT_PTR", "UINT_PTR", "LONG_PTR", "ULONG_PTR", "SIZE_T", "SSIZE_T")],
        "offsets": [],
    },
    "winnt": {
        "header": "um/winnt.h",
        # windows.h normally selects the architecture and supplies these prerequisites.
        "includes": "#include <excpt.h>\n#include <minwindef.h>\n#include <winnt.h>\n",
        "architecture_define": True,
        "layouts": [("DWORD", 4, 4), ("WCHAR", 2, 2), ("LARGE_INTEGER", 8, 8), ("ULARGE_INTEGER", 8, 8)],
        "offsets": [],
    },
    "windows": {
        "header": "um/windows.h",
        "includes": "#define WIN32_LEAN_AND_MEAN 1\n#include <windows.h>\n",
        "layouts": [("DWORD", 4, 4), ("SIZE_T", 8, 8), ("WCHAR", 2, 2), ("FILETIME", 8, 4), ("SYSTEMTIME", 16, 2), ("POINT", 8, 4), ("RECT", 16, 4)],
        "offsets": [("FILETIME", "dwHighDateTime", 4), ("SYSTEMTIME", "wMilliseconds", 14), ("POINT", "y", 4), ("RECT", "right", 8), ("RECT", "bottom", 12)],
    },
}


def target_guard(target: str) -> str:
    profile = TARGETS[target]
    return f"#if {profile['guard']}\n#error expected Windows {profile['vcvars'].upper()}\n#endif\n"


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def sdk_configuration(environment: dict[str, str]) -> tuple[list[Path], Path]:
    """Preserve vcvarsall's search order and require its selected SDK version."""
    directories = [Path(part.strip().strip('"')).resolve() for part in environment.get("INCLUDE", "").split(";") if part.strip()]
    if not directories or len(directories) > 64 or any(not path.is_dir() for path in directories):
        raise RuntimeError("vcvarsall INCLUDE must contain existing include directories")
    version = environment.get("WindowsSDKVersion", "").rstrip("\\/")
    root = environment.get("WindowsSdkDir", "")
    if not root or not re.fullmatch(r"\d+\.\d+\.\d+\.\d+", version):
        raise RuntimeError("vcvarsall did not select a Windows SDK root and version")
    sdk = (Path(root) / "Include" / version).resolve(strict=True)
    ucrt_version = environment.get("UCRTVersion", version).rstrip("\\/")
    if not re.fullmatch(r"\d+\.\d+\.\d+\.\d+", ucrt_version):
        raise RuntimeError("vcvarsall selected an invalid UCRT version")
    ucrt = Path(environment.get("UniversalCRTSdkDir", root)) / "Include" / ucrt_version / "ucrt"
    for component, path in (("shared", sdk / "shared"), ("um", sdk / "um"), ("ucrt", ucrt)):
        if path.resolve(strict=True) not in directories:
            raise RuntimeError(f"vcvarsall INCLUDE lacks the selected SDK {component} directory")
    return directories, sdk


def c_source(case: dict, target: str = TARGET) -> str:
    checks = [f'_Static_assert(sizeof({name}) == {size} && _Alignof({name}) == {align}, "{name}");' for name, size, align in case["layouts"]]
    checks.extend(f'_Static_assert(offsetof({name}, {field}) == {offset}, "{name}.{field}");' for name, field, offset in case["offsets"])
    architecture = f"#define {TARGETS[target]['define']} 1\n" if case.get("architecture_define") else ""
    return target_guard(target) + architecture + case["includes"] + "#include <stddef.h>\n" + "\n".join(checks) + "\n"


def rust_source(case: dict) -> str:
    checks = [f"assert_eq!((size_of::<{name}>(), align_of::<{name}>()), ({size}, {align}));" for name, size, align in case["layouts"]]
    checks.extend(f"assert_eq!(::core::mem::offset_of!({name}, {field}), {offset});" for name, field, offset in case["offsets"])
    return '#![allow(dead_code, non_camel_case_types, non_snake_case, non_upper_case_globals)]\ninclude!("bindings.rs");\nfn main() {\nuse ::core::mem::{size_of, align_of};\n' + "\n".join(checks) + '\nprintln!("SDK layouts passed");\n}\n'


def machine(path: Path) -> int:
    """Read the machine field of a COFF object, bigobj, or PE executable."""
    with path.open("rb") as stream:
        header = stream.read(64)
        if header[:2] == b"MZ" and len(header) == 64:
            stream.seek(int.from_bytes(header[60:64], "little"))
            header = stream.read(6)
            if header[:4] != b"PE\0\0":
                raise RuntimeError(f"missing PE signature: {path}")
            header = header[4:]
        elif header[:4] == b"\0\0\xff\xff":
            header = header[6:8]
        if len(header) < 2:
            raise RuntimeError(f"missing COFF machine field: {path}")
        return int.from_bytes(header[:2], "little")


def completed(stages: dict) -> bool:
    return all(stages.get(name, {}).get("status") == "passed" for name in REQUIRED_STAGES)


class Runner:
    def __init__(self, output: Path, evidence: dict) -> None:
        self.output = output
        self.evidence = evidence
        self.deadline = time.monotonic() + RUN_SECONDS

    def run(self, command: list[str | Path], name: str) -> dict:
        entry = {"command": list(map(str, command)), "status": "failed", "exit_code": None}
        self.evidence["commands"][name] = entry
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            entry["error"] = "SDK check exceeded its 330-second deadline"
            return entry
        stdout, stderr = (self.output / f"{name}.{suffix}" for suffix in ("stdout", "stderr"))
        start = time.monotonic()
        try:
            with stdout.open("wb") as out, stderr.open("wb") as err:
                process = subprocess.Popen(entry["command"], cwd=self.output, stdout=out, stderr=err)
                try:
                    entry["exit_code"] = process.wait(timeout=min(COMMAND_SECONDS, remaining))
                    if entry["exit_code"] == 0:
                        entry["status"] = "passed"
                except subprocess.TimeoutExpired:
                    entry["error"] = "command timed out"
                    try:
                        if sys.platform == "win32":
                            subprocess.run(["taskkill.exe", "/PID", str(process.pid), "/T", "/F"], stdout=out, stderr=err, timeout=10, check=False)
                    finally:
                        process.kill()
                        process.wait(timeout=10)
        except (OSError, subprocess.TimeoutExpired) as error:
            entry["error"] = str(error)
        entry["elapsed_seconds"] = time.monotonic() - start
        entry["logs"] = {path.name: {"bytes": path.stat().st_size, "sha256": digest(path)} for path in (stdout, stderr) if path.exists()}
        print(f"{name}: {entry['status']} (exit {entry['exit_code']})", flush=True)
        if entry["status"] != "passed":
            for path in (stdout, stderr):
                if path.exists():
                    print(path.read_text(encoding="utf-8", errors="replace")[-2000:], flush=True)
        return entry


def require(entry: dict, message: str) -> None:
    if entry["status"] != "passed":
        raise RuntimeError(message)


def record_inputs(paths: list[Path], inputs: dict[str, str]) -> None:
    if len(paths) > 4096:
        raise RuntimeError("SDK dependency list exceeds 4096 paths")
    for path in paths:
        path = path.resolve(strict=True)
        if not path.is_file():
            raise RuntimeError(f"SDK dependency is not a file: {path}")
        actual = digest(path)
        if str(path) in inputs and inputs[str(path)] != actual:
            raise RuntimeError(f"SDK input changed: {path}")
        inputs[str(path)] = actual


def contains_selected_header(dependencies: list[Path], selected: Path) -> bool:
    """Match file identity across Windows extended-length and ordinary paths."""
    return any(path.is_file() and path.samefile(selected) for path in dependencies)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--clang-cl", default=os.environ.get("TOUCAN_CLANG_CL", "clang-cl.exe"))
    parser.add_argument("--target", choices=TARGETS, default=TARGET)
    args = parser.parse_args()
    selected = TARGETS[args.target]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    evidence = {"schema_version": 1, "status": "failed", "target": args.target, "native_execution": False, "architecture": platform.machine(), "python": sys.version, "commit": os.environ.get("GITHUB_SHA"), "commands": {}, "cases": {}, "input_sha256": {}}
    runner = Runner(output, evidence)
    try:
        if sys.platform != "win32":
            raise RuntimeError(f"native Windows {selected['vcvars'].upper()} is required")
        require(runner.run(["rustc", "--version", "--verbose"], "rustc-version"), "rustc version failed")
        rustc = (output / "rustc-version.stdout").read_text(encoding="utf-8", errors="replace")
        if not re.search(r"^host: " + re.escape(args.target) + r"\s*$", rustc, re.MULTILINE):
            raise RuntimeError(f"native Windows {selected['vcvars'].upper()} Rust toolchain is required")
        if os.environ.get("VSCMD_ARG_TGT_ARCH", "").lower() != selected["vcvars"]:
            raise RuntimeError(f"vcvarsall must select {selected['vcvars']}")
        evidence["environment"] = {name: os.environ.get(name) for name in ("INCLUDE", "WindowsSdkDir", "WindowsSDKVersion", "UniversalCRTSdkDir", "UCRTVersion", "VCToolsInstallDir", "VCToolsVersion", "VisualStudioVersion", "VSCMD_ARG_HOST_ARCH", "VSCMD_ARG_TGT_ARCH")}
        directories, sdk = sdk_configuration(os.environ)
        evidence["include_dirs"] = list(map(str, directories))
        evidence["sdk"] = str(sdk)
        evidence["tools"] = {}
        for name in ("cl.exe", args.clang_cl, "link.exe", "rustc"):
            executable = shutil.which(name)
            if not executable:
                raise RuntimeError(f"required tool is missing: {name}")
            evidence["tools"][name] = {"path": executable, "sha256": digest(Path(executable))}
        toucan = args.toucan.resolve(strict=True)
        evidence["toucan"] = {"path": str(toucan), "sha256": digest(toucan)}
        common = ["--target", args.target, "--compiler", "clang", "--std", "c11", "--max-tokens", "2000000"]
        includes = [arg for path in directories for arg in ("-I", str(path))]
        profile = output / "profile.c"
        profile.write_text(target_guard(args.target) + '#define S1(x) #x\n#define S(x) S1(x)\n' + "\n".join(f'const char *profile_{name} = S({name});' for name in ("_MSC_VER", "_MSC_FULL_VER", "__clang_major__", "_M_ARM64", "_M_X64", "_WIN64", "__STDC_VERSION__")), encoding="utf-8")
        for name, command in (
            ("msvc-version", ["cl.exe", "/Bv", "/std:c11", "/TC", "/c", profile, f"/Fo{output / 'profile.obj'}"]),
            ("clang-version", [args.clang_cl, "--version"]),
            ("msvc-profile", ["cl.exe", "/nologo", "/std:c11", "/TC", "/EP", profile]),
            ("clang-profile", [args.clang_cl, "--target=" + args.target, "/nologo", "/std:c11", "/TC", "/EP", profile]),
            ("toucan-profile", [toucan, "preprocess", profile, *common]),
        ):
            require(runner.run(command, name), f"tool/profile control failed: {name}")
        for name, case in CASES.items():
            directory = output / name
            directory.mkdir()
            source = directory / "sdk.c"
            source.write_text(c_source(case, args.target), encoding="utf-8")
            primary = (sdk / case["header"]).resolve(strict=True)
            record_inputs([primary, source], evidence["input_sha256"])
            row = {"primary_header": str(primary), "status": "failed", "stages": {}, "layouts": case["layouts"], "offsets": case["offsets"]}
            evidence["cases"][name] = row
            stages = row["stages"]
            try:
                dependency_file = directory / "msvc-dependencies.json"
                for compiler, executable, flags in (("msvc", "cl.exe", ["/sourceDependencies", dependency_file]), ("clang", args.clang_cl, ["--target=" + args.target])):
                    obj = directory / f"{compiler}.obj"
                    stage = runner.run([executable, "/nologo", "/std:c11", "/TC", "/c", *flags, source, f"/Fo{obj}"], f"{name}-{compiler}")
                    stages[compiler] = stage
                    if stage["status"] == "passed":
                        stage["coff_machine"] = machine(obj)
                        if stage["coff_machine"] != selected["machine"]:
                            stage.update(status="failed", error=f"C oracle did not emit {selected['vcvars'].upper()} COFF")
                if stages["msvc"]["status"] == "passed":
                    dependencies = json.loads(dependency_file.read_text(encoding="utf-8-sig"))["Data"]["Includes"]
                    native_paths = [Path(path).resolve(strict=True) for path in dependencies]
                    if not contains_selected_header(native_paths, primary):
                        raise RuntimeError("MSVC dependency report omitted the selected SDK header")
                    record_inputs(native_paths, evidence["input_sha256"])
                stages["preprocess"] = runner.run([toucan, "preprocess", source, *common, *includes, "--output", directory / "preprocessed.i"], f"{name}-preprocess")
                require(stages["preprocess"], "Toucan preprocessing failed")
                stages["check"] = runner.run([toucan, "check", source, *common, *includes], f"{name}-check")
                require(stages["check"], "Toucan declaration/body analysis failed")
                report = directory / "bindings-report.json"
                bindings = directory / "bindings.rs"
                selectors = [arg for item, _, _ in case["layouts"] for arg in ("--allowlist", item)]
                stages["bindgen"] = runner.run([toucan, "bindgen", source, *common, *includes, *selectors, "--report", report, "--output", bindings], f"{name}-bindgen")
                require(stages["bindgen"], "Toucan binding generation failed")
                metadata = json.loads(report.read_text(encoding="utf-8"))
                if (metadata["target"], metadata["compiler"], metadata["language_mode"]) != (args.target, "clang", "c11"):
                    raise RuntimeError("unexpected Toucan target or compiler profile")
                paths = [Path(path).resolve() for path in metadata["dependencies"]]
                if not contains_selected_header(paths, primary):
                    raise RuntimeError("Toucan dependency report omitted the selected SDK header")
                row["non_file_dependencies"] = [str(path) for path in paths if not path.is_file()]
                record_inputs([path for path in paths if path.is_file()], evidence["input_sha256"])
                row["bindings_sha256"] = digest(bindings)
                consumer = directory / "consumer.rs"
                consumer.write_text(rust_source(case), encoding="utf-8")
                executable = directory / "consumer.exe"
                stages["rustc"] = runner.run(["rustc", "--edition=2021", "--target", args.target, consumer, "-o", executable], f"{name}-rustc")
                require(stages["rustc"], "generated Rust did not compile")
                if machine(executable) != selected["machine"]:
                    raise RuntimeError(f"Rust consumer is not a {selected['vcvars'].upper()} executable")
                stages["execute"] = runner.run([executable], f"{name}-execute")
                if completed(stages):
                    row["status"] = "passed"
            except (OSError, RuntimeError, ValueError, KeyError) as error:
                row["error"] = str(error)
            finally:
                for stage in REQUIRED_STAGES:
                    stages.setdefault(stage, {"status": "not_run", "reason": "an earlier prerequisite failed"})
                print(f"{name}: {row['status']} {row.get('error', '')}", flush=True)
        evidence["changed_inputs"] = [path for path, expected in evidence["input_sha256"].items() if not Path(path).is_file() or digest(Path(path)) != expected]
        evidence["inputs_unchanged"] = not evidence["changed_inputs"]
        if evidence["inputs_unchanged"] and len(evidence["cases"]) == len(CASES) and all(row["status"] == "passed" for row in evidence["cases"].values()):
            evidence.update(status="passed", native_execution=True)
    except (OSError, RuntimeError, ValueError, KeyError) as error:
        evidence["error"] = str(error)
    finally:
        evidence["elapsed_seconds"] = RUN_SECONDS - (runner.deadline - time.monotonic())
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
    print(f"Windows {selected['vcvars'].upper()} SDK check: {evidence['status']}", flush=True)
    return 0 if evidence["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
