#!/usr/bin/env python3
"""Compare actual default and Toucan-feature application builds without libclang."""

from __future__ import annotations

import argparse
import ctypes.util
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from verify_astral_builder import (
    binary_artifact,
    check_features,
    check_lock_versions,
    generated_artifacts,
)
from verify_astral_consumers import (
    binding_artifacts,
    check_archive_source,
    compare_runtime,
    digest,
    library_test_executable,
    library_test_results,
    patch_zstd_lockfile,
    run,
    source_inventory,
)

TARGET = "x86_64-unknown-linux-gnu"
INTEGRATION = ROOT / "corpus/consumers/zstd-optin"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def no_libclang() -> dict:
    """Check the library search cache and installed library directories."""
    found = set()
    roots = {Path(p).resolve() for p in ("/usr/lib", "/usr/local/lib", "/lib", "/opt")}
    for root in roots:
        if root.exists():
            found.update(
                str(path)
                for path in root.rglob("libclang*")
                if not path.name.startswith("libclang_rt")
                and (path.is_file() or path.is_symlink())
            )
    lookup = ctypes.util.find_library("clang")
    require(
        not found and lookup is None,
        f"libclang is available: {sorted(found)}, {lookup}",
    )
    return {
        "searched_roots": sorted(map(str, roots)),
        "library_lookup": lookup,
        "found": [],
    }


def download(url: str, destination: Path, expected: str, offline: bool) -> None:
    if not destination.exists():
        require(not offline, f"missing cached archive: {destination}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        temporary = destination.with_suffix(destination.suffix + ".download")
        with (
            urllib.request.urlopen(url, timeout=120) as response,
            temporary.open("wb") as output,
        ):
            shutil.copyfileobj(response, output)
        require(digest(temporary) == expected, f"archive checksum mismatch: {url}")
        temporary.rename(destination)
    require(
        digest(destination) == expected,
        f"cached archive checksum mismatch: {destination}",
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", choices=["ty", "uv"], required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rust-toolchain")
    parser.add_argument("--build-timeout", type=int, default=1800)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    require(
        __debug__,
        "run without Python optimization so imported validation checks remain active",
    )
    require(args.build_timeout > 0, "build timeout must be positive")
    args.cache, args.output = args.cache.resolve(), args.output.resolve()
    require(not args.output.exists(), "use a fresh output directory")
    work = args.cache / (args.project + "-work")
    target = args.cache / (args.project + "-target")
    require(
        not work.exists() and not target.exists(),
        "use fresh project work and target directories",
    )
    require(
        not work.is_relative_to(ROOT),
        "keep scratch project sources outside the Toucan checkout",
    )
    require(
        not any(c.isspace() for c in str(args.cache) + str(args.output) + str(ROOT)),
        "dep-info audit requires paths without whitespace",
    )
    require(
        "ZSTD_SYS_USE_PKG_CONFIG" not in os.environ,
        "unset ZSTD_SYS_USE_PKG_CONFIG to test bundled C sources",
    )
    args.output.mkdir(parents=True)
    name = args.project
    project = json.loads((ROOT / "corpus/consumers/astral.json").read_text())[name]
    commands = []
    result = {
        "status": "failed",
        "mode": "application-toucan-zstd-feature",
        "project": project,
        "target": TARGET,
        "platform": platform.platform(),
        "python_version": sys.version,
        "driver_sha256": digest(Path(__file__)),
        "commands": commands,
    }
    environment = os.environ | {"CARGO_TARGET_DIR": str(target)}
    if args.rust_toolchain:
        environment["RUSTUP_TOOLCHAIN"] = args.rust_toolchain
    cargo_prefix = ["cargo"] + (
        ["+" + args.rust_toolchain] if args.rust_toolchain else []
    )
    rustc_prefix = ["rustc"] + (
        ["+" + args.rust_toolchain] if args.rust_toolchain else []
    )

    def command(argv: list[str], cwd: Path, label: str) -> Path:
        stdout, stderr = (
            args.output / (label + ".stdout"),
            args.output / (label + ".stderr"),
        )
        entry = {
            "command": argv,
            "cwd": str(cwd),
            "stdout": str(stdout),
            "stderr": str(stderr),
        }
        commands.append(entry)
        with stdout.open("w") as out, stderr.open("w") as err:
            try:
                completed = subprocess.run(
                    argv,
                    cwd=cwd,
                    env=environment,
                    stdout=out,
                    stderr=err,
                    timeout=args.build_timeout,
                    check=False,
                )
                entry["exit_code"] = completed.returncode
            except subprocess.TimeoutExpired:
                entry["timed_out"] = True
                raise
            finally:
                out.flush()
                err.flush()
                entry["stdout_sha256"] = digest(stdout)
                entry["stderr_sha256"] = digest(stderr)
        require(completed.returncode == 0, f"{label} failed; see {stderr}")
        return stdout

    def cargo(arguments: list[str], source: Path, label: str) -> Path:
        return command(cargo_prefix + arguments, source, label)

    def tree(source: Path, options: list[str], generated: bool, label: str) -> dict:
        path = cargo(
            [
                "tree",
                "--target",
                TARGET,
                "--edges",
                "normal,build",
                "--prefix",
                "none",
                "--format",
                "{p}|{f}",
                "-p",
                name,
                *options,
            ],
            source,
            label,
        )
        names = {line.split(" v")[0] for line in path.read_text().splitlines()}
        require(
            not names & {"bindgen", "clang-sys"},
            f"{label} selects a libclang generator",
        )
        require(
            ("toucan_bindgen" in names) == generated,
            f"unexpected generator selection in {label}",
        )
        return {"path": str(path), "sha256": digest(path), "packages": sorted(names)}

    def patch_project(source: Path, reverse: bool, label: str) -> None:
        patch = INTEGRATION / "patches" / (name + ".patch")
        base = ["git", "-C", str(source), "apply"] + (["--reverse"] if reverse else [])
        command(base + ["--check", str(patch)], source, label + "-check")
        command(base + [str(patch)], source, label)

    def libraries(
        source: Path, options: list[str], generated: bool, replacement: Path, label: str
    ) -> list[dict]:
        results = []
        for spec in project["library_tests"]:
            package = spec["package"]
            features = list(spec["features"])
            if generated:
                features.append(package + "/toucan-zstd")
            arguments = [
                "test",
                "--locked",
                "--target",
                TARGET,
                "--no-run",
                "--lib",
                "-p",
                package,
                "--message-format=json-render-diagnostics",
            ]
            if features:
                arguments += ["--features", ",".join(features)]
            log = cargo(arguments + options, source, label + "-" + package)
            executable = library_test_executable(log, package)
            execution = run(
                [str(executable), "--test-threads=1", "--color=never"],
                source / "crates" / package,
                timeout=args.build_timeout,
                env=environment,
            )
            output = args.output / (label + "-" + package + "-deps")
            bindings = (
                generated_artifacts(
                    log, replacement, output, TARGET, generator_feature="toucan"
                )
                if generated
                else binding_artifacts(log, None, output)
            )
            parsed = library_test_results(execution["stdout"])
            require(
                parsed["passed"] == {"ty_vendored": 2, "uv-extract": 19}[package],
                "pinned test count changed",
            )
            results.append(
                {
                    "package": package,
                    "execution": execution,
                    "result": parsed,
                    "bindings": bindings,
                    "executable_sha256": digest(executable),
                }
            )
        return results

    try:
        version = command(
            rustc_prefix + ["--version", "--verbose"], ROOT, "rustc-version"
        ).read_text()
        require(
            f"host: {TARGET}\n" in version,
            "the opt-in gate requires native Linux x86-64",
        )
        result["rustc_version"] = version
        result["cargo_version"] = command(
            cargo_prefix + ["--version"], ROOT, "cargo-version"
        ).read_text()
        if not args.prepare_only:
            result["libclang_before"] = no_libclang()
            result["cc_version"] = command(
                ["cc", "--version"], ROOT, "cc-version"
            ).read_text()
        spec = importlib.util.spec_from_file_location(
            "optin_prepare", INTEGRATION / "prepare.py"
        )
        preparation_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(preparation_module)
        archives = args.cache / "archives"
        for package, version, checksum, _ in preparation_module.PACKAGES:
            filename = f"{package}-{version}.crate"
            download(
                f"https://static.crates.io/crates/{package}/{filename}",
                archives / filename,
                checksum,
                args.offline,
            )
        project_archive = archives / (name + ".tar.gz")
        download(
            f"https://codeload.github.com/{project['repository']}/tar.gz/{project['revision']}",
            project_archive,
            project["archive_sha256"],
            args.offline,
        )
        prepare_args = [
            sys.executable,
            "-B",
            str(INTEGRATION / "prepare.py"),
            "--work-dir",
            str(work),
            "--crate-cache",
            str(archives),
            "--toucan-source",
            str(ROOT),
            "--ruff-archive" if name == "ty" else "--uv-archive",
            str(project_archive),
        ]
        command(prepare_args, ROOT, "prepare")
        preparation = json.loads((work / "preparation.json").read_text())
        result["preparation"] = preparation
        source = Path(preparation["projects"][name]["source"])
        replacement = work / "zstd-rs/zstd-safe/zstd-sys"
        replacement_before = source_inventory(replacement)
        frontend_before = source_inventory(ROOT / "crates")
        patch_project(source, True, "restore-upstream-manifests")
        verified = check_archive_source(project_archive, source)
        verified.pop("archive_lock")
        result["upstream_archive"] = verified
        before = source_inventory(source)
        result["upstream_source_inventory"] = before
        if args.prepare_only:
            result["status"] = "prepared"
            result["application_builds_performed"] = False
            return
        offline = ["--offline"] if args.offline else []
        result["upstream_tree"] = tree(
            source, ["--locked", *offline], False, "upstream-tree"
        )
        common = [
            "build",
            "--locked",
            "--target",
            TARGET,
            "-p",
            name,
            "--message-format=json-render-diagnostics",
            *offline,
        ]
        log = cargo(common, source, "upstream-build")
        upstream_artifact = binary_artifact(log, source, project)
        upstream = args.output / (name + "-upstream")
        shutil.copy2(upstream_artifact["executable"], upstream)
        upstream_bindings = binding_artifacts(log, None, args.output / "upstream-deps")
        upstream_tests = libraries(
            source, offline, False, replacement, "upstream-tests"
        )
        shutil.copyfile(source / "Cargo.lock", args.output / "upstream.lock")
        require(
            source_inventory(source) == before,
            "upstream source changed during default builds",
        )
        patch_project(source, False, "apply-optin-manifests")
        patched = source_inventory(source)
        changed = sorted(
            path
            for path in before.keys() | patched.keys()
            if before.get(path) != patched.get(path)
        )
        require(
            changed and all(path.endswith("Cargo.toml") for path in changed),
            "integration edits extend beyond project manifests",
        )
        config = [
            "--config",
            "patch.crates-io.zstd-sys.path=" + json.dumps(str(replacement)),
        ]
        feature = ["--features", name + "/toucan-zstd"]
        patch_zstd_lockfile(source / "Cargo.lock")
        result["toucan_tree"] = tree(
            source, config + feature + offline, True, "resolve-toucan-tree"
        )
        result["lock"] = check_lock_versions(
            args.output / "upstream.lock", source / "Cargo.lock"
        )
        generated_start = time.time_ns()
        log = cargo(common + config + feature, source, "toucan-build")
        generated_artifact = binary_artifact(log, source, project)
        generated = args.output / (name + "-toucan")
        shutil.copy2(generated_artifact["executable"], generated)
        bindings = generated_artifacts(
            log,
            replacement,
            args.output / "toucan-deps",
            TARGET,
            generator_feature="toucan",
        )
        require(
            len(bindings) == (2 if name == "ty" else 1),
            "missing expected host/runtime zstd-sys instances",
        )
        for binding in bindings:
            path = Path(binding["original_out_dir_binding"])
            require(
                path.is_relative_to(target)
                and path.stat().st_mtime_ns >= generated_start,
                "binding input was not freshly generated in this target directory",
            )
        generated_tests = libraries(
            source, config + offline, True, replacement, "toucan-tests"
        )
        check_features(upstream_bindings, bindings, generator_feature="toucan")
        for old, new in zip(upstream_tests, generated_tests, strict=True):
            require(
                old["result"] == new["result"], "selected library test results changed"
            )
            check_features(old["bindings"], new["bindings"], generator_feature="toucan")
        result["lock"] = check_lock_versions(
            args.output / "upstream.lock", source / "Cargo.lock"
        )
        shutil.copyfile(source / "Cargo.lock", args.output / "toucan.lock")
        after = source_inventory(source)
        require(
            {k: v for k, v in after.items() if k != "Cargo.lock"}
            == {k: v for k, v in patched.items() if k != "Cargo.lock"},
            "project source changed beyond the resolved lockfile",
        )
        require(
            source_inventory(replacement) == replacement_before,
            "patched zstd source changed during builds",
        )
        require(
            source_inventory(ROOT / "crates") == frontend_before,
            "frontend source changed during builds",
        )
        result.update(
            upstream_binary=upstream_artifact,
            toucan_binary=generated_artifact,
            upstream_bindings=upstream_bindings,
            toucan_bindings=bindings,
            library_tests={"upstream": upstream_tests, "toucan": generated_tests},
            changed_project_manifests=changed,
        )
        result["runtime"] = compare_runtime(
            name, upstream, generated, args.output / "runtime", sys.executable
        )
        result["libclang_after"] = no_libclang()
        result["status"] = "passed"
    except Exception as error:
        result["error"] = str(error)
        raise
    finally:
        (args.output / "evidence.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
