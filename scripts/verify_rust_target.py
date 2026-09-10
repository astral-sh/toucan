#!/usr/bin/env python3
"""Compile and execute every generated corpus layout assertion with Rust 1.64."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import re
import subprocess
from pathlib import Path

if not __debug__:
    raise RuntimeError(
        "Validation requires Python assertions; unset PYTHONOPTIMIZE and omit -O."
    )

ROOT = Path(__file__).resolve().parents[1]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepared", type=Path, required=True)
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rust-toolchain", default="1.64.0")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    commands = []
    evidence = {
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "target": args.target,
        "rust_target": "1.64",
        "toucan_sha256": digest(args.toucan),
        "prepared_sha256": digest(args.prepared),
        "commands": commands,
        "projects": [],
        "status": "failed",
    }

    def run(command: list[str], directory: Path, name: str) -> str:
        stdout, stderr = directory / f"{name}.stdout", directory / f"{name}.stderr"
        entry = {"command": command, "stdout": str(stdout), "stderr": str(stderr)}
        commands.append(entry)
        with stdout.open("w") as out, stderr.open("w") as err:
            result = subprocess.run(
                command, cwd=ROOT, stdout=out, stderr=err, timeout=180, check=False
            )
        entry["exit_code"] = result.returncode
        if result.returncode:
            raise RuntimeError(f"{name} failed: {stderr.read_text()[-6000:]}")
        return stdout.read_text()

    try:
        rustc = ["rustup", "run", args.rust_toolchain, "rustc"]
        evidence["rustc"] = run(
            [*rustc, "--version", "--verbose"], output, "rustc"
        ).strip()
        projects = json.loads(args.prepared.read_text())["projects"]
        assert {project["name"] for project in projects} == {
            "zlib",
            "sqlite",
            "zstd",
            "libgit2",
        }
        for project in projects:
            directory = output / project["name"]
            directory.mkdir(exist_ok=True)
            bindings = directory / "bindings.rs"
            report = directory / "bindings-report.json"
            command = [
                str(args.toucan.resolve()),
                "bindgen",
                project["header"],
                "--target",
                args.target,
                "--sysroot",
                str(args.sysroot),
                "--rust-target",
                "1.64",
                "--output",
                str(bindings),
                "--report",
                str(report),
            ]
            for include in project["include_dirs"]:
                command.extend(["-I", include])
            for pattern in project["allowlist"]:
                command.extend(["--allowlist", pattern])
            run(command, directory, "generate-bindings")
            source = bindings.read_text()
            metadata = json.loads(report.read_text())
            assert metadata["rust_target"] == "1.64"
            assert "::core::mem::offset_of!" not in source
            expected_tests = source.count("#[test]")
            assert expected_tests > 0
            executable = directory / "layout-tests"
            run(
                [
                    *rustc,
                    "--edition=2021",
                    "--test",
                    "--crate-name",
                    "bindings_layout",
                    "-A",
                    "warnings",
                    "-D",
                    "improper_ctypes",
                    str(bindings),
                    "-o",
                    str(executable),
                ],
                directory,
                "compile-layout-tests",
            )
            result = run([str(executable)], directory, "run-layout-tests")
            match = re.search(r"test result: ok\. (\d+) passed; 0 failed", result)
            assert match and int(match.group(1)) == expected_tests
            evidence["projects"].append(
                {
                    "name": project["name"],
                    "version": project["version"],
                    "commit": project["commit"],
                    "header_sha256": digest(Path(project["header"])),
                    "bindings_sha256": digest(bindings),
                    "report_sha256": digest(report),
                    "layout_tests": expected_tests,
                    "field_offset_assertions": source.count("::core::ptr::addr_of!"),
                    "status": "passed",
                }
            )
            print(f"{project['name']}: {expected_tests} generated layout tests passed")
        assert digest(args.toucan) == evidence["toucan_sha256"]
        evidence["status"] = "passed"
    except Exception as error:
        evidence["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
