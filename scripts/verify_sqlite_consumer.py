#!/usr/bin/env python3
"""Build the pinned SQLite Rust consumer with freshly generated Toucan bindings."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import importlib.util
import json
import os
import platform
import shutil
import subprocess
from pathlib import Path

import tomllib

if not __debug__:
    raise RuntimeError(
        "Validation requires Python assertions; unset PYTHONOPTIMIZE and omit -O."
    )

ROOT = Path(__file__).resolve().parents[1]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package_versions(metadata: dict) -> list[tuple[str, str]]:
    return sorted(
        (package["name"], package["version"]) for package in metadata["packages"]
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument(
        "--cache", type=Path, default=ROOT / "corpus/cache/sqlite-consumer"
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--sysroot", type=Path)
    args = parser.parse_args()
    output, cache = args.output.resolve(), args.cache.resolve()
    output.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    commands = []
    evidence = {
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platform": platform.platform(),
        "target": args.target,
        "toucan_sha256": digest(args.toucan),
        "commands": commands,
        "status": "failed",
    }

    def run(command: list[str], name: str) -> str:
        stdout, stderr = output / f"{name}.stdout", output / f"{name}.stderr"
        entry = {
            "command": command,
            "cwd": str(ROOT),
            "stdout": str(stdout),
            "stderr": str(stderr),
        }
        commands.append(entry)
        with stdout.open("w") as out, stderr.open("w") as err:
            process = subprocess.run(
                command, cwd=ROOT, stdout=out, stderr=err, timeout=900, check=False
            )
        entry["exit_code"] = process.returncode
        if process.returncode:
            raise RuntimeError(f"{name} failed: {stderr.read_text()[-6000:]}")
        return stdout.read_text()

    try:
        evidence["rustc"] = run(["rustc", "--version", "--verbose"], "rustc").strip()
        evidence["cargo"] = run(["cargo", "--version"], "cargo").strip()
        fixture = ROOT / "tools/sqlite_consumer"
        metadata = json.loads(
            run(
                [
                    "cargo",
                    "metadata",
                    "--locked",
                    "--format-version=1",
                    "--manifest-path",
                    str(fixture / "Cargo.toml"),
                ],
                "resolve-upstream",
            )
        )
        packages = {package["name"]: package for package in metadata["packages"]}
        assert packages["rusqlite"]["version"] == "0.40.2"
        assert packages["libsqlite3-sys"]["version"] == "0.38.2"
        assert "bindgen" not in packages, (
            "consumer validation must not depend on libclang"
        )
        sys_source = Path(packages["libsqlite3-sys"]["manifest_path"]).parent
        sys_copy = cache / "libsqlite3-sys"
        shutil.copytree(sys_source, sys_copy, dirs_exist_ok=True)
        consumer = cache / "consumer"
        shutil.copytree(
            fixture,
            consumer,
            dirs_exist_ok=True,
            ignore=shutil.ignore_patterns("target"),
        )
        with (consumer / "Cargo.toml").open("a") as manifest:
            manifest.write(
                f"\n[patch.crates-io]\nlibsqlite3-sys = {{ path = {json.dumps(str(sys_copy))} }}\n"
            )
        # The upstream build script copies this binding input to OUT_DIR.
        # Rust wrapper code, C source, build flags, and linking remain unchanged.
        header = sys_copy / "sqlite3/sqlite3.h"
        bindings = sys_copy / "sqlite3/bindgen_bundled_version.rs"
        generate = [
            str(args.toucan.resolve()),
            "bindgen",
            str(header),
            "--target",
            args.target,
            "--allowlist",
            "sqlite3*",
            "--allowlist",
            "SQLITE*",
            "--generate-cstr",
            "--blocklist-function",
            "sqlite3_auto_extension",
            "--blocklist-function",
            "sqlite3_cancel_auto_extension",
            "--raw-lines-file",
            str(fixture / "auto_extensions.rs"),
            "--output",
            str(bindings),
            "--report",
            str(output / "bindings-report.json"),
        ]
        for pattern in [
            "SQLITE_SERIALIZE_NOCOPY",
            "SQLITE_DESERIALIZE_*",
            "SQLITE_PREPARE_*",
            "SQLITE_TRACE_*",
        ]:
            generate.extend(["--macro-type-for", f"{pattern}=unsigned"])
        if args.sysroot is not None:
            generate.extend(["--sysroot", str(args.sysroot)])
        run(generate, "generate-bindings")
        shutil.copyfile(bindings, output / "bindings.rs")
        # Check every emitted integer/string constant and the existing SQLite
        # layout inventory against native C, independently from wrapper execution.
        spec = importlib.util.spec_from_file_location(
            "verify_corpus", ROOT / "scripts/verify_corpus.py"
        )
        probes = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(probes)
        c, rust, coverage = probes.generate_probes(
            "sqlite",
            header,
            bindings.read_text(),
            json.loads((output / "bindings-report.json").read_text()),
            run_ffi=False,
        )
        (output / "probe.c").write_text(c)
        (output / "probe.rs").write_text(rust)
        c_command = [
            os.environ.get("CC", "cc"),
            "-std=c11",
            str(output / "probe.c"),
            "-o",
            str(output / "c-probe"),
        ]
        if args.sysroot is not None:
            c_command.extend(
                ["-isysroot", str(args.sysroot)]
                if platform.system() == "Darwin"
                else [f"--sysroot={args.sysroot}"]
            )
        run(c_command, "compile-c-probe")
        run(
            [
                "rustc",
                "--edition=2024",
                "-D",
                "improper_ctypes",
                str(output / "probe.rs"),
                "-o",
                str(output / "rust-probe"),
            ],
            "compile-rust-probe",
        )
        c_values = probes.parse_output(run([str(output / "c-probe")], "run-c-probe"))
        rust_values = probes.parse_output(
            run([str(output / "rust-probe")], "run-rust-probe")
        )
        assert c_values == rust_values, {
            key: [c_values.get(key), rust_values.get(key)]
            for key in c_values.keys() | rust_values.keys()
            if c_values.get(key) != rust_values.get(key)
        }
        evidence["c_probe"] = {"coverage": coverage, "comparisons": len(c_values)}
        patched = json.loads(
            run(
                [
                    "cargo",
                    "metadata",
                    "--offline",
                    "--format-version=1",
                    "--manifest-path",
                    str(consumer / "Cargo.toml"),
                ],
                "resolve-generated",
            )
        )
        assert package_versions(metadata) == package_versions(patched), (
            "the consumer dependency versions changed"
        )
        result = run(
            [
                "cargo",
                "run",
                "--release",
                "--locked",
                "--manifest-path",
                str(consumer / "Cargo.toml"),
                "--target-dir",
                str(cache / "target"),
                "-j",
                "4",
            ],
            "run-consumer",
        )
        lock = tomllib.loads((fixture / "Cargo.lock").read_text())
        evidence.update(
            {
                "status": "passed",
                "result": result.strip(),
                "packages": [
                    {
                        key: package[key]
                        for key in ("name", "version", "checksum")
                        if key in package
                    }
                    for package in lock["package"]
                ],
                "fixture_lock_sha256": digest(fixture / "Cargo.lock"),
                "consumer_lock_sha256": digest(consumer / "Cargo.lock"),
                "bindings_sha256": digest(bindings),
                "bindings_report_sha256": digest(output / "bindings-report.json"),
                "upstream_build_script_sha256": digest(sys_source / "build.rs"),
                "tested_build_script_sha256": digest(sys_copy / "build.rs"),
                "headers": {"sqlite3.h": digest(header)},
                "upstream_rust_sha256": digest(sys_source / "src/lib.rs"),
                "tested_rust_sha256": digest(sys_copy / "src/lib.rs"),
                "upstream_c_sha256": digest(sys_source / "sqlite3/sqlite3.c"),
                "tested_c_sha256": digest(sys_copy / "sqlite3/sqlite3.c"),
                "raw_lines_sha256": digest(fixture / "auto_extensions.rs"),
                "upstream_revision": json.loads(
                    (sys_source / ".cargo_vcs_info.json").read_text()
                ),
                "integration_change": "replace libsqlite3-sys bundled binding input; all bindings are unmodified Toucan output including explicit upstream callback overrides",
            }
        )
        print(result.strip())
    except Exception as error:
        evidence["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
