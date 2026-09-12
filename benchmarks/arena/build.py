#!/usr/bin/env python3
"""Build matching timing/allocation binaries against an immutable source revision."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shlex
import shutil
import subprocess
from pathlib import Path

import tomllib


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources(root: Path) -> dict[str, str]:
    paths = [root / "Cargo.toml", root / "Cargo.lock"]
    for crate in sorted((root / "crates").iterdir()):
        if not crate.is_dir():
            continue
        paths.append(crate / "Cargo.toml")
        for directory in [crate / "src", crate / "resources"]:
            if directory.exists():
                paths.extend(path for path in directory.rglob("*") if path.is_file())
    return {
        str(path.relative_to(root)): digest(path)
        for path in sorted(paths)
        if path.is_file()
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, required=True)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--full-arena", action="store_true")
    args = parser.parse_args()
    source = args.source.resolve()
    output = args.output.resolve()
    target = args.target_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    harness = output / "harness"
    template = Path(__file__).resolve().parent
    shutil.copytree(template / "src", harness / "src")
    shutil.copyfile(template / "Cargo.lock", harness / "Cargo.lock")
    manifest = (
        (template / "Cargo.toml")
        .read_text()
        .replace('path = "../../crates/', f'path = "{source}/crates/')
    )
    (harness / "Cargo.toml").write_text(manifest)
    cargo = shlex.split(args.cargo)
    compiler = ["rustc"] + [arg for arg in cargo[1:] if arg.startswith("+")]
    env = dict(os.environ, CARGO_TARGET_DIR=str(target))
    # Resolve only path-package dependency lists; the copied lock retains registry versions.
    subprocess.run(
        cargo
        + [
            "metadata",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
            str(harness / "Cargo.toml"),
        ],
        env=env,
        stdout=subprocess.DEVNULL,
        check=True,
    )
    before = sources(source)
    harness_before = {
        str(path.relative_to(harness)): digest(path)
        for path in sorted(harness.rglob("*"))
        if path.is_file()
    }
    report = {
        "revision": args.revision,
        "source": str(source),
        "source_sha256": before,
        "harness_sha256": harness_before,
        "registry_packages": [
            package
            for package in tomllib.loads((harness / "Cargo.lock").read_text())[
                "package"
            ]
            if package.get("source", "").startswith("registry+")
        ],
        "compiler": subprocess.check_output(
            compiler + ["--version", "--verbose"], text=True
        ),
        "cargo": args.cargo,
        "full_arena": args.full_arena,
        "environment": {
            key: env[key]
            for key in [
                "CARGO_HOME",
                "CARGO_TARGET_DIR",
                "CARGO_BUILD_BUILD_DIR",
                "CARGO_UNSTABLE_OHM_PROC_MACRO_TRUST",
                "CARGO_UNSTABLE_OHM_NATIVE_TOOL_TRUST",
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "RUSTC",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "CARGO_BUILD_TARGET",
                *sorted(
                    key
                    for key in env
                    if key.startswith("CARGO_PROFILE_RELEASE_")
                    or key.endswith("_RUSTFLAGS")
                ),
            ]
            if key in env
        },
        "builds": {},
    }
    (output / "build-before.json").write_text(json.dumps(report, indent=2) + "\n")
    for counting in [False, True]:
        name = "allocations" if counting else "timing"
        features = (["full-arena"] if args.full_arena else []) + (
            ["allocation-counting"] if counting else []
        )
        command = cargo + [
            "build",
            "--offline",
            "--locked",
            "--release",
            "--manifest-path",
            str(harness / "Cargo.toml"),
        ]
        if features:
            command += ["--features", ",".join(features)]
        with (output / f"{name}-build.log").open("w") as log:
            subprocess.run(
                command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True
            )
        binary = output / name
        shutil.copyfile(target / "release/toucan-arena-benchmark", binary)
        binary.chmod(0o755)
        report["builds"][name] = {
            "command": command,
            "path": str(binary),
            "sha256": digest(binary),
        }
        assert sources(source) == before, "source changed while compiling"
        assert {
            str(path.relative_to(harness)): digest(path)
            for path in sorted(harness.rglob("*"))
            if path.is_file()
        } == harness_before, "harness changed while compiling"
    (output / "build.json").write_text(json.dumps(report, indent=2) + "\n")
    print(output / "build.json")


if __name__ == "__main__":
    main()
