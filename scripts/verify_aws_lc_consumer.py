#!/usr/bin/env python3
"""Compare AWS-LC's unchanged Rust wrappers with two generated binding sets."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import tarfile
from pathlib import Path

import tomllib

if not __debug__:
    raise RuntimeError(
        "Validation requires Python assertions; unset PYTHONOPTIMIZE and omit -O."
    )

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tools/aws_lc_consumer"
SYS_FEATURES = ["bindgen", "prebuilt-nasm"]
RS_FEATURES = ["aws-lc-sys", "bindgen", "prebuilt-nasm"]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_hashes(directory: Path) -> dict[str, str]:
    return {
        path.relative_to(directory).as_posix(): digest(path)
        for path in sorted(directory.rglob("*"))
        if path.is_file()
    }


def toucan_sources(directory: Path) -> dict[str, str]:
    paths = [directory / "Cargo.toml", directory / "Cargo.lock"]
    for name in ("crates", "vendor"):
        paths.extend(
            path
            for path in (directory / name).rglob("*")
            if path.is_file() and path.suffix in {".rs", ".toml", ".peg"}
        )
    return {
        path.relative_to(directory).as_posix(): digest(path) for path in sorted(paths)
    }


def active_packages(metadata: dict, tree: str) -> dict[str, dict]:
    # `cargo metadata` includes AWS-LC's inactive weak optional FIPS dependency.
    # Cargo's normal/build tree describes the packages actually selected; the
    # compiler-artifact messages below independently check the built packages.
    selected: dict[tuple[str, str], set[str]] = {}
    for line in tree.splitlines():
        package, features = line.rsplit("|", 1)
        name, version, *_ = package.split()
        selected.setdefault((name, version.removeprefix("v")), set()).update(
            filter(None, features.split(","))
        )
    return {
        package["name"]: package
        | {
            "features": sorted(selected[(package["name"], package["version"])]),
            "versions": sorted(
                version for name, version in selected if name == package["name"]
            ),
        }
        for package in metadata["packages"]
        if (package["name"], package["version"]) in selected
    }


def check_features(packages: dict[str, dict]) -> None:
    assert packages["aws-lc-rs"]["version"] == "1.18.0"
    assert packages["aws-lc-sys"]["version"] == "0.44.0"
    assert packages["aws-lc-rs"]["features"] == RS_FEATURES
    assert packages["aws-lc-sys"]["features"] == SYS_FEATURES
    assert "aws-lc-fips-sys" not in packages


def consumer_dependencies(
    metadata: dict, tree: str, lock: dict, generator: str
) -> dict:
    """Preserve the active consumer graph after removing the one generator edge."""
    active = set()
    for line in tree.splitlines():
        package, _ = line.rsplit("|", 1)
        name, version, *_ = package.split()
        active.add((name, version.removeprefix("v")))
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    locked = {
        (package["name"], package["version"], package.get("source")): package
        for package in lock["package"]
    }
    root = metadata["resolve"]["root"]
    selected, edges, pending, removed = {}, [], [root], []

    def identity(package: dict) -> str:
        return f"{package['name']}@{package['version']}"

    while pending:
        package_id = pending.pop()
        package = packages[package_id]
        key = identity(package)
        if key in selected:
            if selected[key]["id"] != package_id:
                raise RuntimeError(f"ambiguous active package identity: {key}")
            continue
        if (package["name"], package["version"]) not in active:
            raise RuntimeError(f"consumer dependency is absent from Cargo tree: {key}")
        entry = locked.get((package["name"], package["version"], package.get("source")))
        if entry is None:
            raise RuntimeError(f"consumer dependency is absent from Cargo lock: {key}")
        copied = package_id == root or package["name"] == "aws-lc-sys"
        selected[key] = {
            "id": package_id,
            # These two sources are independently checked byte-for-byte. Their
            # local manifest substitutions deliberately change Cargo's source ID.
            "source": None if copied else package.get("source"),
            "checksum": None if copied else entry.get("checksum"),
            "features": sorted(nodes[package_id]["features"]),
        }
        for dependency in nodes[package_id]["deps"]:
            kinds = [
                kind
                for kind in dependency["dep_kinds"]
                if kind["kind"] in (None, "build")
            ]
            if not kinds:
                continue
            target = packages[dependency["pkg"]]
            if (target["name"], target["version"]) not in active:
                continue
            if package["name"] == "aws-lc-sys" and dependency["name"] == "bindgen":
                if target["name"] != generator or any(
                    kind["kind"] != "build" for kind in kinds
                ):
                    raise RuntimeError("unexpected AWS-LC binding-generator edge")
                removed.append(identity(target))
                continue
            edges.append(
                {
                    "from": key,
                    "to": identity(target),
                    "name": dependency["name"],
                    "kinds": sorted(
                        kinds, key=lambda kind: json.dumps(kind, sort_keys=True)
                    ),
                }
            )
            pending.append(dependency["pkg"])
    if len(removed) != 1:
        raise RuntimeError("expected one active AWS-LC binding-generator edge")
    for package in selected.values():
        del package["id"]
    return {
        "packages": dict(sorted(selected.items())),
        "edges": sorted(edges, key=lambda edge: json.dumps(edge, sort_keys=True)),
    }


def check_consumer_dependencies(expected: dict, actual: dict) -> None:
    if expected != actual:
        raise RuntimeError(
            "non-generator package identities, features, or edges changed"
        )


def verify_registry_source(source: Path) -> dict:
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    archive = (
        cargo_home / "registry/cache" / source.parent.name / f"{source.name}.crate"
    )
    lock = tomllib.loads((FIXTURE / "Cargo.lock").read_text())
    package = next(
        package
        for package in lock["package"]
        if f"{package['name']}-{package['version']}" == source.name
    )
    assert digest(archive) == package["checksum"], (
        "registry archive does not match the Cargo lock"
    )
    files = set()
    with tarfile.open(archive) as handle:
        for member in handle:
            if not member.isfile():
                continue
            relative = Path(member.name).relative_to(source.name)
            assert ".." not in relative.parts
            contents = handle.extractfile(member)
            assert contents is not None
            expected = hashlib.sha256(contents.read()).hexdigest()
            assert digest(source / relative) == expected, (
                f"modified registry source: {relative}"
            )
            files.add(relative.as_posix())
    hashes = source_hashes(source)
    assert hashes.keys() - files <= {".cargo-ok", ".cargo-checksum.json"}, (
        "unexpected files in registry source"
    )
    return {
        "package_checksum": package["checksum"],
        "verified_files": len(files),
        "files": hashes,
    }


def verify(args: argparse.Namespace) -> dict:
    cache, output = args.cache.resolve(), args.output.resolve()
    cache.mkdir(parents=True, exist_ok=True)
    output.mkdir(parents=True, exist_ok=True)
    if any(cache.iterdir()) or any(output.iterdir()):
        raise RuntimeError("use fresh, empty cache and output directories")
    if any(character.isspace() for character in str(cache)):
        raise RuntimeError(
            "the strict Cargo dep-info check requires a cache path without whitespace"
        )
    commands: list[dict] = []
    evidence = {
        "status": "failed",
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platform": platform.platform(),
        "execution": "native",
        "generated_rust_target": "1.70",
        "wrapper_minimum_rust_version": "1.71",
        "commands": commands,
        "cases": {},
        "harness_sha256": digest(Path(__file__)),
    }
    environment = os.environ.copy()

    def run(command: list[str], name: str, extra_env: dict | None = None) -> str:
        stdout, stderr = output / f"{name}.stdout", output / f"{name}.stderr"
        entry = {
            "command": command,
            "cwd": str(ROOT),
            "stdout": str(stdout),
            "stderr": str(stderr),
        }
        if extra_env:
            entry["environment"] = extra_env
        commands.append(entry)
        with stdout.open("w") as out, stderr.open("w") as err:
            result = subprocess.run(
                command,
                cwd=ROOT,
                env=environment | (extra_env or {}),
                stdout=out,
                stderr=err,
                check=False,
                timeout=1800,
            )
        entry.update(
            exit_code=result.returncode,
            stdout_sha256=digest(stdout),
            stderr_sha256=digest(stderr),
        )
        if result.returncode:
            raise RuntimeError(f"{name} failed: {stderr.read_text()[-8000:]}")
        return stdout.read_text()

    try:
        # These knobs can silently select other headers, libraries or bindings.
        controlled = {
            name
            for name in environment
            if name.startswith(
                (
                    "AWS_LC_",
                    "BINDGEN_EXTRA_CLANG_ARGS",
                    "CFLAGS",
                    "CXXFLAGS",
                    "RUSTFLAGS",
                    "CARGO_ENCODED_RUSTFLAGS",
                    "CC_",
                    "CARGO_TARGET_",
                )
            )
            or name
            in {
                "HOST_CC",
                "TARGET_CC",
                "HOST_CFLAGS",
                "TARGET_CFLAGS",
                "CMAKE_TOOLCHAIN_FILE",
            }
        }
        if controlled:
            raise RuntimeError(
                f"unset build overrides before paired validation: {sorted(controlled)}"
            )
        cc = shutil.which(args.cc)
        if cc is None:
            raise RuntimeError(f"C compiler not found: {args.cc}")
        evidence["rustc"] = run(["rustc", "--version", "--verbose"], "rustc")
        evidence["cargo"] = run(["cargo", "--version"], "cargo")
        target = next(
            line.removeprefix("host: ")
            for line in evidence["rustc"].splitlines()
            if line.startswith("host: ")
        )
        if not target.endswith(("-linux-gnu", "-apple-darwin")):
            raise RuntimeError(
                "this harness currently executes native GNU/Linux and macOS consumers"
            )
        evidence["target"] = target
        evidence["cc"] = run([cc, "--version"], "cc")
        if "clang" not in evidence["cc"].lower():
            raise RuntimeError(
                "use Clang so the explicit resource headers and native oracle agree"
            )
        resource = Path(
            run([cc, "-print-resource-dir"], "clang-resource").strip()
        ).resolve()
        clang_args = [
            "-I",
            str(resource / "include"),
            "--sysroot",
            str(args.sysroot.resolve()),
        ]
        environment.update(
            CC=cc,
            CFLAGS=shlex.join(clang_args),
            CARGO_INCREMENTAL="0",
            BINDGEN_EXTRA_CLANG_ARGS=shlex.join(clang_args),
            LIBCLANG_PATH=str(args.libclang_dir.resolve()),
            CC_ENABLE_DEBUG_OUTPUT="1",
            AWS_LC_SYS_EXTERNAL_BINDGEN="0",
        )
        environment.pop("CARGO_TARGET_DIR", None)
        evidence["paired_environment"] = {
            name: environment[name]
            for name in (
                "CC",
                "CFLAGS",
                "CARGO_INCREMENTAL",
                "BINDGEN_EXTRA_CLANG_ARGS",
                "LIBCLANG_PATH",
                "CC_ENABLE_DEBUG_OUTPUT",
                "AWS_LC_SYS_EXTERNAL_BINDGEN",
            )
        }
        evidence["libclang_libraries"] = {
            str(path): digest(path)
            for path in sorted(args.libclang_dir.glob("libclang.*"))
            if path.is_file()
        }
        assert evidence["libclang_libraries"], "libclang shared library was not found"
        metadata_args = [
            "metadata",
            "--locked",
            "--offline",
            "--format-version=1",
            "--filter-platform",
            target,
        ]
        tree_args = [
            "tree",
            "--locked",
            "--offline",
            "--target",
            target,
            "--prefix",
            "none",
            "--format",
            "{p}|{f}",
            "-e",
            "normal,build",
            "--no-dedupe",
        ]
        original = json.loads(
            run(
                [
                    "cargo",
                    *metadata_args,
                    "--manifest-path",
                    str(FIXTURE / "Cargo.toml"),
                ],
                "upstream-metadata",
            )
        )
        original_tree = run(
            ["cargo", *tree_args, "--manifest-path", str(FIXTURE / "Cargo.toml")],
            "upstream-tree",
        )
        packages = active_packages(original, original_tree)
        original_dependencies = consumer_dependencies(
            original,
            original_tree,
            tomllib.loads((FIXTURE / "Cargo.lock").read_text()),
            "bindgen",
        )
        evidence["non_generator_dependencies"] = original_dependencies
        check_features(packages)
        assert packages["bindgen"]["version"] == "0.72.1"
        sys_source = Path(packages["aws-lc-sys"]["manifest_path"]).parent
        rs_source = Path(packages["aws-lc-rs"]["manifest_path"]).parent
        evidence["upstream_sources"] = {
            "aws-lc-sys": verify_registry_source(sys_source),
            "aws-lc-rs": verify_registry_source(rs_source),
        }
        evidence["fixture_source_sha256"] = source_hashes(FIXTURE)
        candidate_source = args.toucan_source.resolve()
        if not args.reference_only:
            evidence["toucan_source_sha256"] = toucan_sources(candidate_source)
        for case in ["reference"] if args.reference_only else ["reference", "toucan"]:
            directory = cache / case
            directory.mkdir()
            sys_copy, consumer, target_dir = (
                directory / "aws-lc-sys",
                directory / "consumer",
                directory / "target",
            )
            shutil.copytree(sys_source, sys_copy)
            shutil.copytree(FIXTURE, consumer)
            if case == "toucan":
                manifest = sys_copy / "Cargo.toml"
                before = '[build-dependencies.bindgen]\nversion = "0.72.0"'
                replacement = (
                    '[build-dependencies.bindgen]\npackage = "toucan_bindgen"\npath = '
                    + json.dumps(str(candidate_source / "crates/toucan_bindgen"))
                )
                text = manifest.read_text()
                assert text.count(before) == 1
                manifest.write_text(text.replace(before, replacement))
            manifest = consumer / "Cargo.toml"
            with manifest.open("a") as handle:
                handle.write(
                    "\n[patch.crates-io]\naws-lc-sys = { path = "
                    + json.dumps(str(sys_copy))
                    + " }\n"
                )
            # Preserve the fixture lock's selected versions while replacing only
            # the sys package and the candidate generator's dependency closure.
            run(
                [
                    "cargo",
                    "metadata",
                    "--offline",
                    "--format-version=1",
                    "--filter-platform",
                    target,
                    "--manifest-path",
                    str(manifest),
                ],
                f"{case}-resolve",
            )
            metadata = json.loads(
                run(
                    ["cargo", *metadata_args, "--manifest-path", str(manifest)],
                    f"{case}-metadata",
                )
            )
            selected_tree = run(
                ["cargo", *tree_args, "--manifest-path", str(manifest)],
                f"{case}-tree",
            )
            selected = active_packages(metadata, selected_tree)
            check_features(selected)
            assert Path(selected["aws-lc-sys"]["manifest_path"]).parent == sys_copy
            assert Path(selected["aws-lc-rs"]["manifest_path"]).parent == rs_source
            dependencies = consumer_dependencies(
                metadata,
                selected_tree,
                tomllib.loads((consumer / "Cargo.lock").read_text()),
                "toucan_bindgen" if case == "toucan" else "bindgen",
            )
            check_consumer_dependencies(original_dependencies, dependencies)
            if case == "toucan":
                assert "toucan_bindgen" in selected
                assert not {"bindgen", "clang-sys", "libloading"}.intersection(selected)
                assert (
                    Path(selected["toucan_bindgen"]["manifest_path"]).parent
                    == candidate_source / "crates/toucan_bindgen"
                )
            else:
                assert selected["bindgen"]["version"] == "0.72.1"
            case_evidence = {
                "non_generator_dependencies": dependencies,
                "packages": {
                    name: {
                        key: package[key]
                        for key in ("versions", "features", "manifest_path")
                    }
                    for name, package in sorted(selected.items())
                },
                "lock_sha256": digest(consumer / "Cargo.lock"),
                "manifest_sha256": digest(manifest),
            }
            evidence["cases"][case] = case_evidence
            shutil.copyfile(consumer / "Cargo.lock", output / f"{case}-Cargo.lock")
            messages = [
                json.loads(line)
                for line in run(
                    [
                        "cargo",
                        "build",
                        "--release",
                        "--locked",
                        "--offline",
                        "--target",
                        target,
                        "--manifest-path",
                        str(manifest),
                        "--target-dir",
                        str(target_dir),
                        "--message-format=json",
                        "-j",
                        str(args.jobs),
                    ],
                    f"{case}-build",
                ).splitlines()
            ]
            sys_id = selected["aws-lc-sys"]["id"]
            builds = [
                message
                for message in messages
                if message["reason"] == "build-script-executed"
                and message["package_id"] == sys_id
            ]
            assert len(builds) == 1
            build = builds[0]
            assert "use_bindgen_pregenerated" in build["cfgs"]
            case_evidence["build_script"] = build
            out_dir = Path(build["out_dir"])
            assert "static=aws_lc_0_44_0_crypto" in build["linked_libs"]
            native_paths = [
                Path(path.removeprefix("native=")) for path in build["linked_paths"]
            ]
            linked_archives = [
                path / "libaws_lc_0_44_0_crypto.a"
                for path in native_paths
                if path.is_relative_to(out_dir)
            ]
            assert any(path.is_file() for path in linked_archives), (
                "bundled archive is missing from the native link search path"
            )
            binding = out_dir / "bindings.rs"
            binding_text = binding.read_text()
            marker = (
                "Generated by Toucan for " + target
                if case == "toucan"
                else "automatically generated by rust-bindgen 0.72.1"
            )
            assert marker in binding_text, "unexpected bindings generator"
            assert "aws_lc_0_44_0_SHA256" in binding_text
            dep_info = []
            for dep in sorted(
                (target_dir / target / "release/deps").glob("aws_lc_sys-*.d")
            ):
                words = dep.read_text().split()
                if str(sys_copy / "src/lib.rs") in words:
                    assert str(binding) in words, (
                        "compiled sys module did not consume OUT_DIR/bindings.rs"
                    )
                    assert not any(word.endswith("_crypto.rs") for word in words), (
                        "pregenerated source was consumed"
                    )
                    dep_info.append(dep)
            assert len(dep_info) == 1
            for source, destination in (
                (binding, output / f"{case}-bindings.rs"),
                (dep_info[0], output / f"{case}-sys.d"),
            ):
                shutil.copyfile(source, destination)
            case_evidence["bindings_sha256"] = digest(binding)
            case_evidence["dep_info_sha256"] = digest(dep_info[0])
            for name in ("output", "stderr"):
                source = out_dir.parent / name
                assert source.is_file()
                shutil.copyfile(source, output / f"{case}-build-script.{name}")
            artifacts = [
                message
                for message in messages
                if message["reason"] == "compiler-artifact"
            ]
            compiled_ids = {message["package_id"] for message in artifacts}
            compiled_packages = sorted(
                (package["name"], package["version"])
                for package in metadata["packages"]
                if package["id"] in compiled_ids
            )
            case_evidence["compiled_packages"] = compiled_packages
            assert "aws-lc-fips-sys" not in {name for name, _ in compiled_packages}
            if case == "toucan":
                assert not {"bindgen", "clang-sys", "libloading"}.intersection(
                    name for name, _ in compiled_packages
                )
            binaries = [
                Path(message["executable"])
                for message in artifacts
                if message["target"]["name"] == "toucan-aws-lc-consumer"
                and message.get("executable")
            ]
            assert len(binaries) == 1
            executable = output / f"{case}-consumer"
            shutil.copyfile(binaries[0], executable)
            executable.chmod(0o755)
            case_evidence["consumer_sha256"] = digest(executable)
            runtime_artifacts = output / f"{case}-artifacts"
            runtime_artifacts.mkdir()
            result = run(
                [str(executable)],
                f"{case}-run",
                {"TOUCAN_CONSUMER_OUTPUT": str(runtime_artifacts)},
            )
            case_evidence["result"] = result
            case_evidence["runtime_artifacts"] = source_hashes(runtime_artifacts)
            assert len(case_evidence["runtime_artifacts"]) == 41
            c_executable = output / f"{case}-c-layout"
            includes = [
                sys_copy / path
                for path in ("include", "generated-include", "aws-lc/include")
            ]
            run(
                [
                    cc,
                    "-std=c11",
                    "-O2",
                    *clang_args,
                    *[arg for path in includes for arg in ("-I", str(path))],
                    str(consumer / "layout.c"),
                    "-o",
                    str(c_executable),
                ],
                f"{case}-c-layout-build",
            )
            c_layouts = run([str(c_executable)], f"{case}-c-layout-run")
            rust_layouts = "".join(
                line + "\n"
                for line in result.splitlines()
                if line.startswith("layout ")
            )
            assert c_layouts == rust_layouts, "C and Rust public layouts differ"
            case_evidence["c_layout_sha256"] = digest(c_executable)
            test_messages = [
                json.loads(line)
                for line in run(
                    [
                        "cargo",
                        "test",
                        "--no-run",
                        "--release",
                        "--locked",
                        "--offline",
                        "--target",
                        target,
                        "--manifest-path",
                        str(manifest),
                        "--target-dir",
                        str(target_dir),
                        "-p",
                        "aws-lc-sys",
                        "--lib",
                        "--message-format=json",
                        "-j",
                        str(args.jobs),
                    ],
                    f"{case}-layout-test-build",
                ).splitlines()
            ]
            test_artifacts = [
                message
                for message in test_messages
                if message["reason"] == "compiler-artifact"
                and message["package_id"] == sys_id
                and message.get("executable")
            ]
            assert len(test_artifacts) == 1
            assert sorted(test_artifacts[0]["features"]) == SYS_FEATURES
            test_builds = [
                message
                for message in test_messages
                if message["reason"] == "build-script-executed"
                and message["package_id"] == sys_id
            ]
            assert len(test_builds) == 1 and test_builds[0]["out_dir"] == str(out_dir)
            assert "use_bindgen_pregenerated" in test_builds[0]["cfgs"]
            assert digest(binding) == case_evidence["bindings_sha256"], (
                "test build changed the generated bindings"
            )
            layout_test = output / f"{case}-layout-tests"
            shutil.copyfile(test_artifacts[0]["executable"], layout_test)
            layout_test.chmod(0o755)
            test_result = run([str(layout_test)], f"{case}-layout-test-run")
            summary = re.search(
                r"test result: ok\. (\d+) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out",
                test_result,
            )
            assert summary and int(summary[1]) >= 6, (
                "generated layout tests did not execute"
            )
            case_evidence["generated_layout_tests"] = int(summary[1])
            case_evidence["layout_test_sha256"] = digest(layout_test)
            tested = source_hashes(sys_copy)
            original_hashes = evidence["upstream_sources"]["aws-lc-sys"]["files"]
            changed = sorted(
                name
                for name in original_hashes.keys() | tested.keys()
                if original_hashes.get(name) != tested.get(name)
            )
            assert changed == (["Cargo.toml"] if case == "toucan" else []), (
                f"modified upstream files: {changed}"
            )
            case_evidence["changed_upstream_files"] = changed
            case_evidence["sys_source_sha256"] = tested
            native_archives = sorted(out_dir.rglob("*.a"))
            assert native_archives, "bundled native archives were not built"
            case_evidence["native_archives"] = {
                path.relative_to(out_dir).as_posix(): digest(path)
                for path in native_archives
            }
            case_evidence["native_object_files"] = sorted(
                path.relative_to(out_dir).as_posix() for path in out_dir.rglob("*.o")
            )
            native_configuration = {}
            for path in sorted(out_dir.rglob("*")):
                if path.name not in {"CMakeCache.txt", "flags.make"}:
                    continue
                relative = path.relative_to(out_dir).as_posix()
                text = path.read_text()
                if path.name == "CMakeCache.txt":
                    # Keep compiler identity, flags and source paths while
                    # excluding unrelated CMake discovery cache entries.
                    text = "\n".join(
                        line
                        for line in text.splitlines()
                        if line.startswith(
                            ("CMAKE_C_", "CMAKE_ASM_", "CMAKE_BUILD_TYPE:")
                        )
                    )
                normalized = text.replace(str(sys_copy), "$AWS_LC_SYS").replace(
                    str(out_dir), "$OUT_DIR"
                )
                native_configuration[relative] = normalized
            case_evidence["native_configuration"] = native_configuration
        assert (
            source_hashes(rs_source)
            == evidence["upstream_sources"]["aws-lc-rs"]["files"]
        )
        assert source_hashes(FIXTURE) == evidence["fixture_source_sha256"]
        assert digest(Path(__file__)) == evidence["harness_sha256"]
        if not args.reference_only:
            assert toucan_sources(candidate_source) == evidence["toucan_source_sha256"]
            reference, candidate = (
                evidence["cases"][name] for name in ("reference", "toucan")
            )
            assert reference["result"] == candidate["result"]
            assert reference["runtime_artifacts"] == candidate["runtime_artifacts"]
            assert (
                reference["native_object_files"] == candidate["native_object_files"]
            ), "native source selection differs"
            assert (
                reference["native_archives"].keys()
                == candidate["native_archives"].keys()
            )
            assert (
                reference["native_configuration"] == candidate["native_configuration"]
            ), "native compiler flags or includes differ"
            evidence["status"] = "passed"
        else:
            evidence["status"] = "reference_passed"
    except Exception as error:
        evidence["error"] = str(error)
        raise
    finally:
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    return evidence


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cc", required=True, help="native Clang executable")
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--libclang-dir", type=Path, required=True)
    parser.add_argument("--toucan-source", type=Path, default=ROOT)
    parser.add_argument(
        "--reference-only",
        action="store_true",
        help="validate the reference fixture without claiming Toucan parity",
    )
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    result = verify(args)
    print(result["status"])


if __name__ == "__main__":
    main()
