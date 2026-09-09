#!/usr/bin/env python3
"""Build pinned ty/uv using zstd-sys's unchanged build.rs and the Toucan builder."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_astral_consumers import (
    binding_artifacts,
    check_archive_source,
    compare_runtime,
    digest,
    library_test_executable,
    library_test_results,
    patch_zstd_lockfile,
    prepare_project,
    run,
    source_inventory,
)

ROOT = Path(__file__).resolve().parents[1]


def lock_packages(path: Path) -> dict[tuple[str, str], dict]:
    packages = tomllib.loads(path.read_text())["package"]
    result = {(p["name"], p["version"]): p for p in packages}
    if len(result) != len(packages):
        raise RuntimeError("duplicate package name/version in lockfile")
    return result


def dependency_identity(
    text: str, packages: dict[tuple[str, str], dict]
) -> tuple[str, str, str | None]:
    parts = text.split()
    candidates = [
        p
        for p in packages.values()
        if p["name"] == parts[0] and (len(parts) == 1 or p["version"] == parts[1])
    ]
    if len(candidates) != 1:
        raise RuntimeError(f"ambiguous lock dependency: {text}")
    package = candidates[0]
    source = None if package["name"] == "zstd-sys" else package.get("source")
    return package["name"], package["version"], source


def check_lock_versions(before: Path, after: Path) -> dict:
    original, tested = lock_packages(before), lock_packages(after)
    missing = original.keys() - tested.keys()
    if missing:
        raise RuntimeError(
            f"existing locked versions removed or changed: {sorted(missing)}"
        )
    changed = []
    for identity, package in original.items():
        actual = tested[identity]
        if identity[0] == "zstd-sys":
            if "source" in actual or "checksum" in actual:
                raise RuntimeError(
                    "zstd-sys was not replaced by the explicit local source"
                )
        else:
            for key in ("source", "checksum"):
                if actual.get(key) != package.get(key):
                    raise RuntimeError(
                        f"existing package identity changed: {identity} {key}"
                    )
        previous = {
            dependency_identity(edge, original)
            for edge in package.get("dependencies", [])
        }
        current = {
            dependency_identity(edge, tested) for edge in actual.get("dependencies", [])
        }
        if previous - current:
            raise RuntimeError(
                f"existing dependency versions changed: {identity}: {previous - current}"
            )
        additions = current - previous
        if additions:
            changed.append(
                {"package": identity, "added_dependencies": sorted(additions)}
            )
    return {
        "upstream_sha256": digest(before),
        "builder_sha256": digest(after),
        "existing_packages_preserved": len(original),
        "added_packages": [
            tested[key] for key in sorted(tested.keys() - original.keys())
        ],
        "existing_dependency_edge_additions": changed,
    }


def prepare_zstd(args: argparse.Namespace) -> dict:
    command = [
        "cargo",
        "metadata",
        "--locked",
        "--offline",
        "--format-version=1",
        "--manifest-path",
        str(ROOT / "tools/zstd_consumer/Cargo.toml"),
    ]
    metadata = json.loads(run(command, ROOT, timeout=args.build_timeout)["stdout"])
    package = next(p for p in metadata["packages"] if p["name"] == "zstd-sys")
    if package["version"] != "2.0.16+zstd.1.5.7" or not package["id"].startswith(
        "registry+"
    ):
        raise RuntimeError("expected the pinned registry zstd-sys package")
    original = Path(package["manifest_path"]).parent
    replacement = args.zstd_source or args.output / "zstd-sys"
    before = source_inventory(original)
    text = (original / "Cargo.toml").read_text()
    dependency = '[build-dependencies.bindgen]\nversion = "0.72"'
    rewritten = (
        '[build-dependencies.bindgen]\npackage = "toucan_bindgen"\npath = '
        + json.dumps(str(ROOT / "crates/toucan_bindgen"))
    )
    if text.count(dependency) != 1 or text.count("std = []") != 1:
        raise RuntimeError(
            "pinned zstd-sys manifest no longer matches the reviewed changes"
        )
    manifest = text.replace(dependency, rewritten).replace(
        "std = []", 'std = ["bindgen"]'
    )
    expected = before | {"Cargo.toml": hashlib.sha256(manifest.encode()).hexdigest()}
    if args.zstd_source is None:
        if replacement.exists():
            raise RuntimeError(
                "use a fresh output directory; replacement source already exists"
            )
        shutil.copytree(original, replacement)
        (replacement / "Cargo.toml").write_text(manifest)
    elif not replacement.is_dir():
        raise RuntimeError("--zstd-source must name an existing prepared package")
    if source_inventory(replacement) != expected:
        raise RuntimeError(
            "prepared zstd-sys source differs from the pinned package and exact manifest edits"
        )
    after = source_inventory(replacement)
    changed = sorted(
        name
        for name in before.keys() | after.keys()
        if before.get(name) != after.get(name)
    )
    if changed != ["Cargo.toml"]:
        raise RuntimeError(f"zstd-sys changes extend beyond its manifest: {changed}")
    return {
        "source": str(original),
        "replacement": str(replacement),
        "reused_source": args.zstd_source is not None,
        "version": package["version"],
        "manifest_changes": [
            "Alias the bindgen build-dependency to toucan_bindgen",
            "Activate bindgen from the std feature, already selected by both consumers",
        ],
        "changed_files": changed,
        "build_script_sha256": digest(replacement / "build.rs"),
        "original_inventory": before,
        "replacement_inventory": after,
    }


def binary_artifact(log: Path, source: Path, project: dict) -> dict:
    """Select the executable Cargo built, including configured target directories."""
    selected = []
    for line in log.read_text().splitlines():
        artifact = json.loads(line)
        if (
            artifact.get("reason") != "compiler-artifact"
            or artifact["target"]["name"] != project["binary"]
            or "bin" not in artifact["target"]["kind"]
            or artifact["profile"]["test"]
            or artifact["executable"] is None
        ):
            continue
        manifest = Path(artifact["manifest_path"]).resolve()
        if not manifest.is_relative_to(source.resolve()):
            raise RuntimeError("selected project executable belongs to another source")
        if tomllib.loads(manifest.read_text())["package"]["name"] != project["package"]:
            raise RuntimeError("selected project executable belongs to another package")
        executable = Path(artifact["executable"])
        if not executable.is_file():
            raise RuntimeError("Cargo-selected project executable is missing")
        selected.append(
            {
                "executable": str(executable),
                "sha256": digest(executable),
                "manifest_path": str(manifest),
                "package_id": artifact["package_id"],
                "profile": artifact["profile"],
            }
        )
    if len(selected) != 1:
        raise RuntimeError(
            f"expected one selected {project['binary']} executable in {log}"
        )
    return selected[0]


def generated_artifacts(
    log: Path, source: Path, output: Path, expected_target: str | None = None
) -> list[dict]:
    result = []
    for line in log.read_text().splitlines():
        artifact = json.loads(line)
        if (
            artifact.get("reason") != "compiler-artifact"
            or artifact["target"]["name"] != "zstd_sys"
        ):
            continue
        crate = Path(artifact["manifest_path"]).parent
        if crate.resolve() != source.resolve() or "bindgen" not in artifact["features"]:
            raise RuntimeError(
                "Cargo did not build the explicit source with bindgen enabled"
            )
        rlib = next(Path(p) for p in artifact["filenames"] if p.endswith(".rlib"))
        dep = rlib.with_name(rlib.stem.removeprefix("lib") + ".d")
        words = dep.read_text().split()
        if str(crate / "src/lib.rs") not in words:
            raise RuntimeError("selected zstd-sys dep-info belongs to another source")
        bindings = {
            Path(word)
            for word in words
            if "/out/bindings.rs" in word and not word.endswith(":")
        }
        if len(bindings) != 1:
            raise RuntimeError(f"expected one generated OUT_DIR input in {dep}")
        binding = bindings.pop()
        if expected_target is not None and not binding.read_text().startswith(
            f"// Generated by Toucan for {expected_target}.\n"
        ):
            raise RuntimeError(
                "generated binding target differs from the tested Rust host"
            )
        if "Generated by Toucan" not in binding.read_text():
            raise RuntimeError("selected OUT_DIR bindings were not generated by Toucan")
        if any(
            "/src/bindings_zstd.rs" in word or "/src/bindings_zdict.rs" in word
            for word in words
        ):
            raise RuntimeError("pre-generated binding inputs remain active")
        output.mkdir(parents=True, exist_ok=True)
        retained_dep, retained_binding = (
            output / dep.name,
            output / (rlib.stem + "-bindings.rs"),
        )
        shutil.copyfile(dep, retained_dep)
        shutil.copyfile(binding, retained_binding)
        result.append(
            {
                "package_id": artifact["package_id"],
                "manifest_path": str(crate / "Cargo.toml"),
                "features": artifact["features"],
                "profile": artifact["profile"],
                "dep_info": str(retained_dep),
                "dep_info_sha256": digest(retained_dep),
                "generated_binding": str(retained_binding),
                "generated_binding_sha256": digest(retained_binding),
                "original_out_dir_binding": str(binding),
            }
        )
    if not result:
        raise RuntimeError(f"no selected zstd-sys artifact in {log}")
    return result


def check_features(upstream: list[dict], generated: list[dict]) -> None:
    expected = sorted(sorted(set(row["features"]) | {"bindgen"}) for row in upstream)
    actual = sorted(sorted(row["features"]) for row in generated)
    if expected != actual:
        raise RuntimeError(
            f"feature changes extend beyond bindgen: {expected} != {actual}"
        )


def build_project(
    name: str, project: dict, args: argparse.Namespace, zstd: dict
) -> dict:
    archive, source = prepare_project(project, args.cache, args.offline)
    verified = check_archive_source(archive, source)
    archive_lock = verified.pop("archive_lock")
    output = args.output / name
    output.mkdir()
    original_lock = (source / "Cargo.lock").read_bytes()
    (output / "entry.lock").write_bytes(original_lock)
    before = source_inventory(source)
    target = args.cache / f"{project['repository'].split('/')[1]}-target"
    environment = os.environ | {
        "CARGO_TARGET_DIR": str(target),
        "RUSTUP_TOOLCHAIN": args.rust_toolchain,
    }
    commands = []
    replacement = Path(zstd["replacement"])
    config = [
        "--config",
        f"patch.crates-io.zstd-sys.path={json.dumps(str(replacement))}",
    ]
    result = {
        "status": "failed",
        "project": project,
        "source_before": verified,
        "commands": commands,
        "target_directory": str(target),
    }

    def cargo(arguments: list[str], label: str) -> Path:
        command = ["cargo", *arguments]
        stdout, stderr = (
            output / f"{label}{'.stdout' if label == 'resolve-builder' else '.jsonl'}",
            output / f"{label}.stderr",
        )
        with stdout.open("w") as out, stderr.open("w") as err:
            process = subprocess.run(
                command,
                cwd=source,
                env=environment,
                stdout=out,
                stderr=err,
                timeout=args.build_timeout,
                check=False,
            )
        commands.append(
            {
                "command": command,
                "cwd": str(source),
                "environment": {
                    key: environment[key]
                    for key in ("CARGO_TARGET_DIR", "RUSTUP_TOOLCHAIN", "CARGO_HOME")
                    if key in environment
                },
                "exit_code": process.returncode,
                "stdout": str(stdout),
                "stdout_sha256": digest(stdout),
                "stderr": str(stderr),
                "stderr_sha256": digest(stderr),
            }
        )
        if process.returncode:
            raise RuntimeError(f"{name} {label} failed; see {stderr}")
        return stdout

    def tests(label: str, options: list[str]) -> list[dict]:
        results = []
        for spec in project["library_tests"]:
            package = spec["package"]
            args_cargo = [
                "test",
                "--locked",
                "--offline",
                "--no-run",
                "--lib",
                "-p",
                package,
                "--message-format=json-render-diagnostics",
            ]
            if spec["features"]:
                args_cargo += ["--features", ",".join(spec["features"])]
            log = cargo(args_cargo + options, f"{label}-tests-{package}")
            executable = library_test_executable(log, package)
            execution = run(
                [str(executable), "--test-threads=1", "--color=never"],
                source / "crates" / package,
                timeout=args.build_timeout,
                env=environment,
            )
            artifacts = (
                generated_artifacts(
                    log,
                    replacement,
                    output / f"{label}-tests-{package}-deps",
                    args.target,
                )
                if options
                else binding_artifacts(
                    log, None, output / f"{label}-tests-{package}-deps"
                )
            )
            results.append(
                {
                    "package": package,
                    "features": spec["features"],
                    "executable_sha256": digest(executable),
                    "execution": execution,
                    "result": library_test_results(execution["stdout"]),
                    "bindings": artifacts,
                }
            )
        return results

    try:
        (source / "Cargo.lock").write_bytes(archive_lock)
        common = [
            "build",
            "--locked",
            "--offline",
            "-p",
            project["package"],
            "--message-format=json-render-diagnostics",
        ]
        log = cargo(common, "upstream")
        binaries = output / "bin"
        binaries.mkdir()
        upstream = binaries / (name + "-upstream")
        result["upstream_binary_artifact"] = binary_artifact(log, source, project)
        shutil.copy2(result["upstream_binary_artifact"]["executable"], upstream)
        shutil.copyfile(source / "Cargo.lock", output / "upstream.lock")
        upstream_bindings = binding_artifacts(log, None, output / "upstream-deps")
        upstream_tests = tests("upstream", [])
        patch_zstd_lockfile(source / "Cargo.lock")
        cargo(
            [
                "tree",
                "--offline",
                "-p",
                project["package"],
                "--edges",
                "normal,build",
                "--target",
                args.target,
                *config,
            ],
            "resolve-builder",
        )
        result["lock"] = check_lock_versions(
            output / "upstream.lock", source / "Cargo.lock"
        )
        log = cargo(common + config, "builder")
        generated = binaries / (name + "-builder")
        result["builder_binary_artifact"] = binary_artifact(log, source, project)
        shutil.copy2(result["builder_binary_artifact"]["executable"], generated)
        generated_bindings = generated_artifacts(
            log, replacement, output / "builder-deps", args.target
        )
        generated_tests = tests("builder", config)
        check_features(upstream_bindings, generated_bindings)
        for old, new in zip(upstream_tests, generated_tests, strict=True):
            if old["result"] != new["result"]:
                raise RuntimeError("library test results changed")
            check_features(old["bindings"], new["bindings"])
            expected_passed = {"ty_vendored": 2, "uv-extract": 19}[old["package"]]
            if new["result"]["passed"] != expected_passed:
                raise RuntimeError("pinned library test count changed")
        result["lock"] = check_lock_versions(
            output / "upstream.lock", source / "Cargo.lock"
        )
        shutil.copyfile(source / "Cargo.lock", output / "builder.lock")
        after = source_inventory(source)
        changed = sorted(
            key
            for key in before.keys() | after.keys()
            if before.get(key) != after.get(key)
        )
        if set(changed) - {"Cargo.lock"}:
            raise RuntimeError(
                f"project C/Rust/build-script sources changed: {changed}"
            )
        if source_inventory(replacement) != zstd["replacement_inventory"]:
            raise RuntimeError("replacement source changed during the build")
        result.update(
            upstream_bindings=upstream_bindings,
            builder_bindings=generated_bindings,
            library_tests={"upstream": upstream_tests, "builder": generated_tests},
            changed_project_files=changed,
            upstream_binary=str(upstream),
            builder_binary=str(generated),
        )
        result["runtime"] = compare_runtime(
            name, upstream, generated, output / "runtime", args.python
        )
    except Exception as error:
        result["error"] = str(error)
        raise
    finally:
        (source / "Cargo.lock").write_bytes(original_lock)
        restored = source_inventory(source)
        if restored != before:
            raise RuntimeError(
                "project source inventory did not return to its entry state"
            )
        result["entry_inventory_restored"] = True
        (output / "evidence.json").write_text(json.dumps(result, indent=2) + "\n")
    result["status"] = "passed"
    (output / "evidence.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--zstd-source",
        type=Path,
        help="Reuse a previously prepared package after checking its complete source inventory",
    )
    parser.add_argument("--project", choices=["all", "ty", "uv"], default="all")
    parser.add_argument("--rust-toolchain", default="stable")
    parser.add_argument("--build-timeout", type=int, default=1800)
    parser.add_argument("--python", default=sys.executable)
    parser.add_argument(
        "--offline",
        action="store_true",
        help="Require cached archives; Cargo resolution always runs offline",
    )
    args = parser.parse_args()
    if args.build_timeout <= 0:
        parser.error("--build-timeout must be positive")
    args.cache, args.output = args.cache.resolve(), args.output.resolve()
    if args.zstd_source is not None:
        args.zstd_source = args.zstd_source.resolve()
    if args.output.exists():
        parser.error("use a fresh --output directory")
    if any(
        c.isspace()
        for c in str(args.cache)
        + str(args.output)
        + str(ROOT)
        + str(args.zstd_source or "")
    ):
        parser.error("this dep-info audit requires paths without whitespace")
    if "ZSTD_SYS_USE_PKG_CONFIG" in os.environ:
        parser.error(
            "unset ZSTD_SYS_USE_PKG_CONFIG to test the pinned bundled C source"
        )
    args.output.mkdir(parents=True)
    evidence = {
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platform": platform.platform(),
        "status": "failed",
        "mode": "unchanged-build-script",
        "projects": {},
        "driver_sha256": digest(Path(__file__)),
        "frontend_source_sha256": {
            str(p.relative_to(ROOT)): digest(p)
            for p in sorted((ROOT / "crates").rglob("*"))
            if p.is_file() and p.suffix in (".rs", ".toml", ".h")
        },
    }
    try:
        evidence["tools"] = {
            tool: run(
                [tool, "--version", *(["--verbose"] if tool == "rustc" else [])],
                ROOT,
                env=os.environ | {"RUSTUP_TOOLCHAIN": args.rust_toolchain},
            )
            for tool in ["rustc", "cargo", "cc"]
        }
        args.target = next(
            line.removeprefix("host: ")
            for line in evidence["tools"]["rustc"]["stdout"].splitlines()
            if line.startswith("host: ")
        )
        evidence["target"] = args.target
        zstd = prepare_zstd(args)
        evidence["zstd"] = zstd
        projects = json.loads((ROOT / "corpus/consumers/astral.json").read_text())
        for name in projects if args.project == "all" else [args.project]:
            evidence["projects"][name] = build_project(name, projects[name], args, zstd)
        evidence["status"] = "passed"
    except Exception as error:
        evidence["error"] = str(error)
        raise
    finally:
        (args.output / "evidence.json").write_text(
            json.dumps(evidence, indent=2) + "\n"
        )
    print(
        f"{len(evidence['projects'])} pinned projects passed with unchanged build scripts"
    )


if __name__ == "__main__":
    main()
