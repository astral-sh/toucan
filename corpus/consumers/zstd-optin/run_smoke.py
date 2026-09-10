#!/usr/bin/env python3
"""Validate feature selection and consumed bindings in the staged zstd packages."""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from git_source import verify_artifacts, verify_packages

if not __debug__:
    raise RuntimeError(
        "Validation requires Python assertions; unset PYTHONOPTIMIZE and omit -O."
    )

PARSER = argparse.ArgumentParser(description=__doc__)
PARSER.add_argument("--work-dir", type=Path, required=True)
PARSER.add_argument("--target-dir", type=Path, required=True)
PARSER.add_argument("--rust-toolchain")
ARGS = PARSER.parse_args()
if os.environ.get("TOUCAN_GIT_TOKEN"):
    PARSER.error("run smoke builds without the Git fetch token")
ROOT = ARGS.work_dir.resolve()
TARGET = "x86_64-unknown-linux-gnu"
CARGO = ["cargo"] + (["+" + ARGS.rust_toolchain] if ARGS.rust_toolchain else [])
BASE_ENV = os.environ | {"CARGO_INCREMENTAL": "0", "CARGO_BUILD_JOBS": "2"}
if ARGS.rust_toolchain:
    BASE_ENV["RUSTUP_TOOLCHAIN"] = ARGS.rust_toolchain
PREPARATION = json.loads((ROOT / "preparation.json").read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def invoke(command, directory, label, env):
    stdout = directory / (label + ".stdout")
    stderr = directory / (label + ".stderr")
    with stdout.open("w") as out, stderr.open("w") as err:
        result = subprocess.run(
            command, cwd=ROOT, env=env, stdout=out, stderr=err, timeout=600, check=False
        )
    (directory / (label + ".command.json")).write_text(
        json.dumps(command, indent=2) + "\n"
    )
    assert result.returncode == 0, f"{command}: {stderr.read_text()}"
    return stdout


def run_case(name, fixture, features, generated, graphs=1):
    print(f"start {name}", flush=True)
    directory = ROOT / "evidence" / name
    directory.mkdir(parents=True, exist_ok=True)
    env = BASE_ENV | {"CARGO_TARGET_DIR": str(ARGS.target_dir.resolve() / fixture)}
    args = ["--manifest-path", str(ROOT / fixture / "Cargo.toml")]
    if features:
        args += ["--features", ",".join(features)]
    # Resolve the scratch fixture once; following build and graph reads use its lock.
    metadata_path = invoke(
        [
            *CARGO,
            "metadata",
            *args,
            "--offline",
            "--format-version=1",
            "--filter-platform",
            TARGET,
        ],
        directory,
        "metadata",
        env,
    )
    data = json.loads(metadata_path.read_text())
    names = {p["id"]: p["name"] for p in data["packages"]}
    nodes = {n["id"]: n for n in data["resolve"]["nodes"]}
    visited = set()
    todo = [data["resolve"]["root"]]
    while todo:
        node = todo.pop()
        if node in visited:
            continue
        visited.add(node)
        todo += [d["pkg"] for d in nodes[node]["deps"]]
    active = sorted({names[n] for n in visited})
    frontend = None
    if generated:
        source = Path(PREPARATION["toucan_source"])
        frontend = verify_packages(
            data, source, snapshot=PREPARATION["frontend_source"]
        )

    if generated:
        assert "toucan_bindgen" in active
        assert ("bindgen" in active) == ("bindgen" in features)
        assert ("clang-sys" in active) == ("bindgen" in features)
    else:
        assert not {"toucan_bindgen", "bindgen", "clang-sys"} & set(active)
    build_start = time.time_ns()
    build = invoke(
        [
            *CARGO,
            "build",
            *args,
            "--target",
            TARGET,
            "--locked",
            "--offline",
            "--message-format=json",
        ],
        directory,
        "build",
        env,
    )
    frontend_artifacts = verify_artifacts(build, frontend) if frontend else None
    rows = [json.loads(line) for line in build.read_text().splitlines()]
    libs = [
        r
        for r in rows
        if r.get("reason") == "compiler-artifact"
        and r["target"]["name"] == "zstd_sys"
        and "lib" in r["target"]["kind"]
    ]
    assert len(libs) == graphs, f"{name}: expected {graphs} sys graphs, got {len(libs)}"
    evidence = []
    for index, artifact in enumerate(libs):
        library = next(Path(p) for p in artifact["filenames"] if p.endswith(".rlib"))
        dep = library.with_name(
            library.name.removeprefix("lib").removesuffix(".rlib") + ".d"
        )
        text = dep.read_text()
        saved_dep = directory / f"zstd-sys-{index}.d"
        saved_dep.write_text(text)
        info = {
            "features": artifact["features"],
            "library": str(library),
            "library_sha256": digest(library),
            "dep_info": str(saved_dep),
            "cargo_artifact_reused": artifact["fresh"],
        }
        if generated:
            assert "toucan" in artifact["features"]
            assert not any(
                "/" + name in text
                for name in [
                    "bindings_zstd.rs",
                    "bindings_zstd_experimental.rs",
                    "bindings_zdict.rs",
                    "bindings_zdict_experimental.rs",
                ]
            )
            scripts = [
                r
                for r in rows
                if r.get("reason") == "build-script-executed"
                and r["package_id"] == artifact["package_id"]
            ]
            bindings = [
                Path(r["out_dir"]) / "bindings.rs"
                for r in scripts
                if str(Path(r["out_dir"]) / "bindings.rs") in text
            ]
            assert len(bindings) == 1, (name, bindings)
            binding = bindings[0]
            assert binding.read_text().startswith(
                "// Generated by Toucan for " + TARGET + ".\n"
            )
            retained = directory / f"bindings-{index}.rs"
            retained.write_bytes(binding.read_bytes())
            info.update(
                bindings=str(binding),
                bindings_sha256=digest(binding),
                generated_during_this_build=binding.stat().st_mtime_ns >= build_start,
            )
            assert info["generated_during_this_build"], (
                "expected fresh generation, found a cached binding file"
            )
        else:
            assert "/bindings_zstd.rs" in text
            assert (
                "toucan" not in artifact["features"]
                and "bindgen" not in artifact["features"]
            )
        evidence.append(info)
    binary_rows = [
        r
        for r in rows
        if r.get("reason") == "compiler-artifact"
        and r.get("executable")
        and "bin" in r["target"]["kind"]
    ]
    assert len(binary_rows) == 1
    executable = Path(binary_rows[0]["executable"])
    outputs = directory / "runtime-artifacts"
    outputs.mkdir(exist_ok=True)
    runtime = invoke(
        [str(executable)],
        directory,
        "runtime",
        env | {"TOUCAN_CONSUMER_OUTPUT": str(outputs)},
    )
    result = {
        "case": name,
        "active_packages": active,
        "frontend_source": frontend,
        "frontend_artifacts": frontend_artifacts,
        "sys_instances": evidence,
        "executable": str(executable),
        "executable_sha256": digest(executable),
        "runtime_stdout": runtime.read_text(),
        "runtime_artifacts": {p.name: digest(p) for p in sorted(outputs.iterdir())},
        "lock_sha256": digest(ROOT / fixture / "Cargo.lock"),
    }
    (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"pass {name}: {len(libs)} graph(s)", flush=True)
    return result


results = []
for spec in [
    ("default", "consumer", [], False, 1),
    ("toucan", "consumer", ["toucan"], True, 1),
    ("combined", "consumer", ["toucan", "bindgen"], True, 1),
    ("dual-graph", "dual-graph", ["toucan-zstd"], True, 2),
]:
    results.append(run_case(*spec))
for candidate in results[1:3]:
    assert candidate["runtime_stdout"] == results[0]["runtime_stdout"]
    assert candidate["runtime_artifacts"] == results[0]["runtime_artifacts"]
report = {
    "status": "passed",
    "toucan_revision": PREPARATION["frontend_source_revision"],
    "frontend_mode": PREPARATION.get("frontend_mode", "local"),
    "integration_git_revision": PREPARATION["integration_git_revision"],
    "target": TARGET,
    "cases": results,
    "limits": [
        "No full ty/uv rebuild in this smoke run.",
        "No physically libclang-free environment was used.",
        "Optional Toucan generation requires build-host Rust 1.96; old Cargo default-path compatibility remains untested.",
    ],
}
(ROOT / "evidence/summary.json").write_text(json.dumps(report, indent=2) + "\n")
print("all cases passed", flush=True)
