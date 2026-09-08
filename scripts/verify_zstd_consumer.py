#!/usr/bin/env python3
"""Build the pinned zstd Rust consumer with freshly generated Toucan bindings."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import platform
import shutil
import subprocess
from pathlib import Path

import tomllib

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
        "--cache", type=Path, default=ROOT / "corpus/cache/zstd-consumer"
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--sysroot", type=Path)
    parser.add_argument(
        "--rust-toolchain", help="Installed rustup toolchain to test, such as 1.64.0"
    )
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
        rustc = (
            ["rustup", "run", args.rust_toolchain, "rustc"]
            if args.rust_toolchain
            else ["rustc"]
        )
        cargo = (
            ["rustup", "run", args.rust_toolchain, "cargo"]
            if args.rust_toolchain
            else ["cargo"]
        )
        evidence["rust_target"] = "1.64"
        evidence["rustc"] = run([*rustc, "--version", "--verbose"], "rustc").strip()
        evidence["cargo"] = run([*cargo, "--version"], "cargo").strip()
        fixture = ROOT / "tools/zstd_consumer"
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
        assert packages["zstd"]["version"] == "0.13.3"
        assert packages["zstd-safe"]["version"] == "7.2.4"
        assert packages["zstd-sys"]["version"] == "2.0.16+zstd.1.5.7"
        assert "bindgen" not in packages, (
            "consumer validation must not depend on libclang"
        )
        sys_source = Path(packages["zstd-sys"]["manifest_path"]).parent
        sys_copy = cache / "zstd-sys"
        shutil.copytree(sys_source, sys_copy, dirs_exist_ok=True)
        consumer = cache / "consumer"
        shutil.copytree(
            fixture,
            consumer,
            dirs_exist_ok=True,
            ignore=shutil.ignore_patterns("target"),
        )
        if args.rust_toolchain:
            # Modern Cargo fetches the locked sources. Cargo 1.64 then reads only
            # the vendored directory, including on hosts using sparse registries.
            vendor_config = run(
                [
                    "cargo",
                    "vendor",
                    "--locked",
                    "--respect-source-config",
                    "--manifest-path",
                    str(fixture / "Cargo.toml"),
                    str(cache / "vendor"),
                ],
                "vendor-dependencies",
            )
            config = cache / "vendor-config.toml"
            config.write_text(vendor_config)
            cargo.extend(["--config", str(config)])
            evidence["baseline_result"] = run(
                [
                    *cargo,
                    "run",
                    "--release",
                    "--locked",
                    "--offline",
                    "--manifest-path",
                    str(consumer / "Cargo.toml"),
                    "--target-dir",
                    str(cache / "baseline-target"),
                    "-j",
                    "4",
                ],
                "run-upstream-consumer",
            ).strip()
        with (consumer / "Cargo.toml").open("a") as manifest:
            manifest.write(
                f"\n[patch.crates-io]\nzstd-sys = {{ path = {json.dumps(str(sys_copy))} }}\n"
            )
        # Keep upstream C compilation, linking, features, and all wrapper code.
        # Replace the selection of checked-in bindings with this generated file.
        (sys_copy / "src/lib.rs").write_text(
            "#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]\n"
            '#![no_std]\ninclude!("bindings_toucan.rs");\n'
        )
        wrapper = cache / "wrapper.h"
        wrapper.write_text('#include "zstd.h"\n#include "zdict.h"\n')
        bindings = sys_copy / "src/bindings_toucan.rs"
        generate = [
            str(args.toucan.resolve()),
            "bindgen",
            str(wrapper),
            "--target",
            args.target,
            "-I",
            str(sys_copy / "zstd/lib"),
            "--allowlist",
            "ZSTD*",
            "--allowlist",
            "ZDICT*",
            "--rust-target",
            "1.64",
            "--rustified-enums",
            "--size-t-is-usize",
            "--macro-type",
            "unsigned",
            "--output",
            str(bindings),
            "--report",
            str(output / "bindings-report.json"),
        ]
        if args.sysroot is not None:
            generate.extend(["--sysroot", str(args.sysroot)])
        run(generate, "generate-bindings")
        shutil.copyfile(bindings, output / "bindings.rs")
        # Before Rust 1.77 the generated field-offset assertions are tests.
        # Run every assertion with the same compiler used by the consumer.
        layout_tests = output / "layout-tests"
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
                str(layout_tests),
            ],
            "compile-layout-tests",
        )
        evidence["layout_tests"] = run([str(layout_tests)], "run-layout-tests").strip()
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
                *cargo,
                "run",
                "--release",
                "--locked",
                "--offline",
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
                "headers": {
                    name: digest(sys_copy / "zstd/lib" / name)
                    for name in ("zstd.h", "zdict.h", "zstd_errors.h")
                },
                "upstream_revision": json.loads(
                    (sys_source / ".cargo_vcs_info.json").read_text()
                ),
                "integration_change": "replace zstd-sys binding includes; all bindings are unmodified Toucan output",
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
