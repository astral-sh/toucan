#!/usr/bin/env python3
"""Run zstd's untouched bindgen build script with the Toucan builder."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

# Resolve sibling helpers even when the host enables PYTHONSAFEPATH.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_zstd_consumer import PROFILES, ROOT, digest, package_versions, source_hashes


def verify(cache: Path, output: Path, target: str, profiles: list[str]) -> dict:
    cache.mkdir(parents=True, exist_ok=True)
    output.mkdir(parents=True, exist_ok=True)
    commands = []
    evidence = {
        "status": "failed",
        "target": target,
        "commands": commands,
        "profiles": {},
    }

    def run(command: list[str], name: str, artifacts: Path | None = None) -> str:
        stdout, stderr = output / f"{name}.stdout", output / f"{name}.stderr"
        environment = os.environ.copy()
        if artifacts is not None:
            artifacts.mkdir()
            environment["TOUCAN_CONSUMER_OUTPUT"] = str(artifacts)
        entry = {"command": command, "stdout": str(stdout), "stderr": str(stderr)}
        commands.append(entry)
        with stdout.open("w") as out, stderr.open("w") as err:
            process = subprocess.run(
                command,
                cwd=ROOT,
                env=environment,
                stdout=out,
                stderr=err,
                timeout=900,
                check=False,
            )
        entry.update(
            exit_code=process.returncode,
            stdout_sha256=digest(stdout),
            stderr_sha256=digest(stderr),
        )
        if process.returncode:
            raise RuntimeError(f"{name} failed: {stderr.read_text()[-6000:]}")
        return stdout.read_text()

    try:
        if "ZSTD_SYS_USE_PKG_CONFIG" in os.environ:
            raise RuntimeError(
                "unset ZSTD_SYS_USE_PKG_CONFIG to use the pinned bundled library"
            )
        evidence["rustc"] = run(["rustc", "--version", "--verbose"], "rustc")
        if f"host: {target}" not in evidence["rustc"].splitlines():
            raise RuntimeError("--target must match the native Rust host")
        fixture = ROOT / "tools/zstd_consumer"
        original = json.loads(
            run(
                [
                    "cargo",
                    "metadata",
                    "--locked",
                    "--format-version=1",
                    "--manifest-path",
                    str(fixture / "Cargo.toml"),
                ],
                "original-metadata",
            )
        )
        packages = {package["name"]: package for package in original["packages"]}
        assert packages["zstd"]["version"] == "0.13.3"
        assert packages["zstd-safe"]["version"] == "7.2.4"
        assert packages["zstd-sys"]["version"] == "2.0.16+zstd.1.5.7"
        source = Path(packages["zstd-sys"]["manifest_path"]).parent
        sys_copy, baseline, consumer = (
            cache / "zstd-sys",
            cache / "baseline",
            cache / "consumer",
        )
        for directory in (sys_copy, baseline, consumer):
            if directory.exists():
                raise RuntimeError(
                    f"use a fresh cache directory; {directory} already exists"
                )
        shutil.copytree(source, sys_copy)
        for directory in (baseline, consumer):
            shutil.copytree(fixture, directory, ignore=shutil.ignore_patterns("target"))

        manifest = sys_copy / "Cargo.toml"
        previous = '[build-dependencies.bindgen]\nversion = "0.72"'
        replacement = (
            '[build-dependencies.bindgen]\npackage = "toucan_bindgen"\npath = '
            + json.dumps(str(ROOT / "crates/toucan_bindgen"))
        )
        text = manifest.read_text()
        assert text.count(previous) == 1
        manifest.write_text(text.replace(previous, replacement))
        manifest = consumer / "Cargo.toml"
        text = manifest.read_text()
        assert text.count('zstd = "=0.13.3"') == 1
        text = text.replace(
            'zstd = "=0.13.3"', 'zstd = { version = "=0.13.3", features = ["bindgen"] }'
        )
        manifest.write_text(
            text
            + "\n[patch.crates-io]\nzstd-sys = { path = "
            + json.dumps(str(sys_copy))
            + " }\n"
        )
        run(
            [
                "cargo",
                "metadata",
                "--offline",
                "--format-version=1",
                "--manifest-path",
                str(manifest),
            ],
            "resolve-builder",
        )

        for profile in profiles:
            features = PROFILES[profile]
            args = ["--features", ",".join(features)] if features else []
            results = []
            hashes = []
            for name, directory in (("baseline", baseline), ("builder", consumer)):
                artifacts = output / f"{profile}-{name}-artifacts"
                results.append(
                    run(
                        [
                            "cargo",
                            "run",
                            "--release",
                            "--locked",
                            "--offline",
                            "--manifest-path",
                            str(directory / "Cargo.toml"),
                            *args,
                            "-j",
                            "4",
                        ],
                        f"{profile}-{name}",
                        artifacts,
                    )
                )
                hashes.append(source_hashes(artifacts))
            assert results[0] == results[1], f"{profile}: consumer output differs"
            assert hashes[0] == hashes[1], f"{profile}: runtime artifacts differ"
            expected = {"bulk.zst", "stream.zst", "trained.dict", "trained.zst"}
            if "experimental" in features:
                expected.update(("magicless.zst", "cover.dict", "cover.zst"))
            if "zstdmt" in features:
                expected.add("threaded.zst")
                if "experimental" in features:
                    expected.add("shared-pool.zst")
            assert set(hashes[0]) == expected
            for name in expected:
                assert (
                    output / f"{profile}-baseline-artifacts" / name
                ).read_bytes() == (
                    output / f"{profile}-builder-artifacts" / name
                ).read_bytes()
            metadata = json.loads(
                run(
                    [
                        "cargo",
                        "metadata",
                        "--locked",
                        "--offline",
                        "--format-version=1",
                        "--manifest-path",
                        str(consumer / "Cargo.toml"),
                        *args,
                    ],
                    f"{profile}-metadata",
                )
            )
            generated_packages = {
                package["name"]: package for package in metadata["packages"]
            }
            assert "toucan_bindgen" in generated_packages
            assert not {"bindgen", "clang-sys", "libloading"}.intersection(
                generated_packages
            )
            for name, package in packages.items():
                assert generated_packages[name]["version"] == package["version"], (
                    f"dependency version changed: {name}"
                )
            evidence["profiles"][profile] = {
                "status": "passed",
                "result": results[1],
                "identical_runtime_artifacts": hashes[0],
                "packages": package_versions(metadata),
            }

        original_hashes, tested_hashes = source_hashes(source), source_hashes(sys_copy)
        changed = sorted(
            name
            for name in original_hashes.keys() | tested_hashes.keys()
            if original_hashes.get(name) != tested_hashes.get(name)
        )
        assert changed == ["Cargo.toml"], f"unexpected source changes: {changed}"
        dependencies = []
        target_directory = Path(metadata["target_directory"])
        if any(character.isspace() for character in str(target_directory)):
            raise RuntimeError("Cargo target directory must not contain whitespace")
        for path in sorted((target_directory / "release/deps").glob("zstd_sys-*.d")):
            words = path.read_text().split()
            if str(sys_copy / "src/lib.rs") not in words:
                # A shared target directory can contain baseline or unrelated
                # zstd-sys artifacts. Inspect only this run's copied source.
                continue
            # Cargo output directories in this harness cannot contain whitespace:
            # keep this parser strict instead of accepting an incomplete path.
            bindings = {
                Path(word)
                for word in words
                if "/out/bindings.rs" in word and not word.endswith(":")
            }
            for binding in bindings:
                assert (
                    binding.is_file() and "Generated by Toucan" in binding.read_text()
                )
                artifact = output / binding.parent.parent.name
                artifact.mkdir(exist_ok=True)
                shutil.copyfile(binding, artifact / "bindings.rs")
                shutil.copyfile(path, artifact / path.name)
                dependencies.append(
                    {
                        "bindings": str(artifact / "bindings.rs"),
                        "bindings_sha256": digest(binding),
                        "dep_info": str(artifact / path.name),
                        "dep_info_sha256": digest(path),
                    }
                )
        assert len(dependencies) == len(profiles), (
            "every feature build must consume its generated bindings"
        )
        evidence.update(
            status="passed",
            consumed_bindings=dependencies,
            changed_upstream_files=changed,
            build_script_sha256=digest(sys_copy / "build.rs"),
            upstream_source_sha256=original_hashes,
            tested_source_sha256=tested_hashes,
            builder_lock_sha256=digest(consumer / "Cargo.lock"),
        )
    except Exception as error:
        evidence["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    return evidence


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cache", type=Path, required=True, help="Fresh cache directory"
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--profile", action="append", choices=PROFILES)
    args = parser.parse_args()
    profiles = args.profile or list(PROFILES)
    if len(set(profiles)) != len(profiles):
        parser.error("each profile may only be selected once")
    cache, output = args.cache.resolve(), args.output.resolve()
    if any(character.isspace() for character in str(cache)):
        parser.error("cache path must not contain whitespace")
    result = verify(cache, output, args.target, profiles)
    print(f"{len(result['profiles'])} unchanged build-script consumer profiles passed")


if __name__ == "__main__":
    main()
