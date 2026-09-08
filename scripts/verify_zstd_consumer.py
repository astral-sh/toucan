#!/usr/bin/env python3
"""Run pinned zstd Rust consumers with upstream and freshly generated bindings."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import platform
import shutil
import subprocess
from pathlib import Path

import tomllib

ROOT = Path(__file__).resolve().parents[1]
PROFILES = {
    "default": [],
    "experimental": ["experimental"],
    "zstdmt": ["zstdmt"],
    "experimental-zstdmt": ["experimental", "zstdmt"],
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package_versions(metadata: dict) -> list[tuple[str, str]]:
    return sorted(
        (package["name"], package["version"]) for package in metadata["packages"]
    )


def package_features(metadata: dict) -> dict[str, list[str]]:
    names = {package["id"]: package["name"] for package in metadata["packages"]}
    return {
        names[node["id"]]: sorted(node["features"])
        for node in metadata["resolve"]["nodes"]
    }


def source_hashes(directory: Path) -> dict[str, str]:
    return {
        path.relative_to(directory).as_posix(): digest(path)
        for path in sorted(directory.rglob("*"))
        if path.is_file()
    }


def verify(args: argparse.Namespace, profile: str, output: Path, cache: Path) -> dict:
    output.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    features = PROFILES[profile]
    feature_args = ["--features", ",".join(features)] if features else []
    commands: list[dict] = []
    evidence = {
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platform": platform.platform(),
        "target": args.target,
        "profile": profile,
        "features": features,
        "toucan_sha256": digest(args.toucan),
        "commands": commands,
        "status": "failed",
    }

    def run(command: list[str], name: str, artifact_dir: Path | None = None) -> str:
        stdout, stderr = output / f"{name}.stdout", output / f"{name}.stderr"
        entry = {
            "command": command,
            "cwd": str(ROOT),
            "stdout": str(stdout),
            "stderr": str(stderr),
        }
        commands.append(entry)
        environment = None
        if artifact_dir is not None:
            if artifact_dir.exists():
                shutil.rmtree(artifact_dir)
            artifact_dir.mkdir()
            entry["environment"] = {"TOUCAN_CONSUMER_OUTPUT": str(artifact_dir)}
            environment = os.environ | entry["environment"]
        with stdout.open("w") as out, stderr.open("w") as err:
            process = subprocess.run(
                command,
                cwd=ROOT,
                stdout=out,
                stderr=err,
                env=environment,
                timeout=900,
                check=False,
            )
        entry["exit_code"] = process.returncode
        if process.returncode:
            raise RuntimeError(f"{name} failed: {stderr.read_text()[-6000:]}")
        return stdout.read_text()

    try:
        if "ZSTD_SYS_USE_PKG_CONFIG" in os.environ:
            raise RuntimeError(
                "unset ZSTD_SYS_USE_PKG_CONFIG to test the pinned bundled C library"
            )
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
        if f"host: {args.target}" not in evidence["rustc"].splitlines():
            raise RuntimeError(
                "consumer execution requires --target to match the Rust host"
            )
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
                    *feature_args,
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
        upstream_features = package_features(metadata)
        sys_features = upstream_features["zstd-sys"]
        assert "zdict_builder" in sys_features
        for feature in ("experimental", "zstdmt"):
            assert (feature in sys_features) == (feature in features)
        assert not {"bindgen", "pkg-config", "seekable"}.intersection(sys_features)
        evidence["resolved_features"] = upstream_features
        sys_source = Path(packages["zstd-sys"]["manifest_path"]).parent
        sys_copy, consumer = cache / "zstd-sys", cache / "consumer"
        # Start from pristine inputs even when reusing a build cache from an
        # earlier harness version or an interrupted run.
        for path in (sys_copy, consumer):
            if path.exists():
                shutil.rmtree(path)
        shutil.copytree(sys_source, sys_copy)
        shutil.copytree(fixture, consumer, ignore=shutil.ignore_patterns("target"))
        evidence["fixture_source_sha256"] = source_hashes(consumer)
        if args.rust_toolchain:
            # Modern Cargo fetches locked sources; Cargo 1.64 reads only the
            # vendored directory, including on hosts using sparse registries.
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
        run_args = [
            "run",
            "--release",
            "--locked",
            "--offline",
            "--manifest-path",
            str(consumer / "Cargo.toml"),
            *feature_args,
            "-j",
            "4",
        ]
        evidence["baseline_result"] = run(
            [*cargo, *run_args, "--target-dir", str(cache / "baseline-target")],
            "run-upstream-consumer",
            output / "upstream-artifacts",
        ).strip()
        with (consumer / "Cargo.toml").open("a") as manifest:
            manifest.write(
                f"\n[patch.crates-io]\nzstd-sys = {{ path = {json.dumps(str(sys_copy))} }}\n"
            )

        experimental = "experimental" in features
        suffix = "_experimental" if experimental else ""
        selected = [f"src/bindings_{header}{suffix}.rs" for header in ("zstd", "zdict")]
        generated = {}
        for header, relative in zip(("zstd", "zdict"), selected, strict=True):
            bindings = sys_copy / relative
            report = output / f"bindings-{header}-report.json"
            generate = [
                str(args.toucan.resolve()),
                "bindgen",
                str(sys_copy / f"zstd/lib/{header}.h"),
                "--target",
                args.target,
                "--allowlist",
                f"{header.upper()}*",
                "--rust-target",
                "1.64",
                "--rustified-enums",
                "--size-t-is-usize",
                "--helper-namespace",
                header,
                "--macro-type",
                "unsigned",
                "--output",
                str(bindings),
                "--report",
                str(report),
            ]
            if experimental:
                # Match upstream build.rs, including its Rust-specific enum aliases.
                generate.extend(
                    [
                        "-DZSTD_STATIC_LINKING_ONLY",
                        "-DZDICT_STATIC_LINKING_ONLY",
                        "-DZSTD_RUST_BINDINGS_EXPERIMENTAL",
                    ]
                )
            if args.sysroot is not None:
                generate.extend(["--sysroot", str(args.sysroot)])
            run(generate, f"generate-{header}-bindings")
            shutil.copyfile(bindings, output / bindings.name)
            generated[relative] = {
                "sha256": digest(bindings),
                "report_sha256": digest(report),
            }

        # Compile the original crate root: this runs both sets of layout tests
        # and follows exactly the same cfg/include selection as the consumer.
        layout_tests = output / "layout-tests"
        layout_args = ["--cfg", 'feature="zdict_builder"']
        for feature in features:
            layout_args.extend(["--cfg", f'feature="{feature}"'])
        run(
            [
                *rustc,
                "--edition=2018",
                "--test",
                "--crate-name",
                "bindings_layout",
                "-A",
                "warnings",
                "-D",
                "improper_ctypes",
                *layout_args,
                str(sys_copy / "src/lib.rs"),
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
                    *feature_args,
                ],
                "resolve-generated",
            )
        )
        assert package_versions(metadata) == package_versions(patched), (
            "dependency versions changed"
        )
        assert upstream_features == package_features(patched), (
            "dependency features changed"
        )
        fixture_lock = tomllib.loads((fixture / "Cargo.lock").read_text())
        consumer_lock = tomllib.loads((consumer / "Cargo.lock").read_text())
        # A path patch removes only zstd-sys's registry source and checksum.
        for lock in (fixture_lock, consumer_lock):
            for package in lock["package"]:
                if package["name"] == "zstd-sys":
                    package.pop("source", None)
                    package.pop("checksum", None)
        assert fixture_lock == consumer_lock, (
            "locked dependency sources or checksums changed"
        )
        result = run(
            [*cargo, *run_args, "--target-dir", str(cache / "target")],
            "run-consumer",
            output / "toucan-artifacts",
        ).strip()
        evidence["result"] = result
        assert result == evidence["baseline_result"], (
            "consumer results differ from upstream"
        )
        upstream_artifacts = source_hashes(output / "upstream-artifacts")
        generated_artifacts = source_hashes(output / "toucan-artifacts")
        expected_artifacts = {"bulk.zst", "stream.zst", "trained.dict", "trained.zst"}
        if experimental:
            expected_artifacts.update({"magicless.zst", "cover.dict", "cover.zst"})
        if "zstdmt" in features:
            expected_artifacts.add("threaded.zst")
            if experimental:
                expected_artifacts.add("shared-pool.zst")
        assert set(upstream_artifacts) == expected_artifacts, (
            "upstream runtime artifacts missing"
        )
        assert upstream_artifacts == generated_artifacts, (
            "runtime artifact hashes differ"
        )
        for name in upstream_artifacts:
            assert (output / "upstream-artifacts" / name).read_bytes() == (
                output / "toucan-artifacts" / name
            ).read_bytes(), f"runtime artifact bytes differ: {name}"
        evidence["identical_runtime_artifacts"] = upstream_artifacts

        # rustc dep-info proves the cfg-selected includes were consumed. An
        # experimental build that falls back to checked-in bindings must fail.
        dependencies = []
        for path in sorted((cache / "target/release/deps").glob("zstd_sys-*.d")):
            content = path.read_text()
            if all(
                str(sys_copy / name).replace(" ", "\\ ") in content for name in selected
            ):
                copied = output / path.name
                shutil.copyfile(path, copied)
                dependencies.append({"path": str(copied), "sha256": digest(copied)})
        assert dependencies, (
            "rustc dep-info does not contain every generated binding file"
        )
        original_hashes, tested_hashes = (
            source_hashes(sys_source),
            source_hashes(sys_copy),
        )
        changed = sorted(
            name
            for name in original_hashes.keys() | tested_hashes.keys()
            if original_hashes.get(name) != tested_hashes.get(name)
        )
        assert changed == sorted(selected), (
            f"unexpected upstream source changes: {changed}"
        )
        evidence.update(
            {
                "status": "passed",
                "packages": tomllib.loads((fixture / "Cargo.lock").read_text())[
                    "package"
                ],
                "fixture_lock_sha256": digest(fixture / "Cargo.lock"),
                "consumer_lock_sha256": digest(consumer / "Cargo.lock"),
                "generated_bindings": generated,
                "consumed_bindings": dependencies,
                "changed_upstream_files": changed,
                "upstream_source_sha256": original_hashes,
                "tested_source_sha256": tested_hashes,
                "upstream_revision": json.loads(
                    (sys_source / ".cargo_vcs_info.json").read_text()
                ),
                "integration_change": "replace feature-selected bindings with unmodified Toucan output",
            }
        )
        print(f"{profile}: {result}")
    except Exception as error:
        evidence["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    return evidence


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument(
        "--cache", type=Path, default=ROOT / "corpus/cache/zstd-consumer"
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--target", required=True, help="Native Rust host target used for execution"
    )
    parser.add_argument("--sysroot", type=Path)
    parser.add_argument(
        "--rust-toolchain", help="Installed rustup toolchain, such as 1.64.0"
    )
    parser.add_argument(
        "--profile",
        action="append",
        choices=PROFILES,
        help="Feature profile; repeat to run a matrix (default: default)",
    )
    args = parser.parse_args()
    profiles = args.profile or ["default"]
    if len(set(profiles)) != len(profiles):
        parser.error("each --profile may only be selected once")
    output, cache = args.output.resolve(), args.cache.resolve()
    if len(profiles) == 1:
        verify(args, profiles[0], output, cache)
        return
    output.mkdir(parents=True, exist_ok=True)
    matrix = {
        "status": "failed",
        "profiles": {
            profile: {
                "status": "not_run",
                "evidence": str(output / profile / "evidence.json"),
            }
            for profile in profiles
        },
    }
    try:
        for profile in profiles:
            matrix["profiles"][profile]["status"] = "failed"
            result = verify(args, profile, output / profile, cache / profile)
            matrix["profiles"][profile]["status"] = result["status"]
        matrix["status"] = "passed"
    finally:
        (output / "evidence.json").write_text(json.dumps(matrix, indent=2) + "\n")


if __name__ == "__main__":
    main()
