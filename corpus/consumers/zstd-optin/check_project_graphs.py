#!/usr/bin/env python3
"""Resolve staged ty/uv features without compiling either workspace."""

from pathlib import Path
import argparse, os, subprocess, json, tomllib

if not __debug__:
    raise RuntimeError(
        "Validation requires Python assertions; unset PYTHONOPTIMIZE and omit -O."
    )

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--work-dir", type=Path, required=True)
parser.add_argument("--target-dir", type=Path, required=True)
parser.add_argument("--rust-toolchain")
parser.add_argument("--offline", action="store_true")
args = parser.parse_args()
stage = args.work_dir.resolve()
cargo = ["cargo"] + (["+" + args.rust_toolchain] if args.rust_toolchain else [])
preparation = json.loads((stage / "preparation.json").read_text())
summary = {}
for name in ["ty", "uv"]:
    source = Path(preparation["projects"][name]["source"])
    out = stage / "evidence" / (name + "-graph")
    out.mkdir(parents=True, exist_ok=True)
    env = os.environ | {"CARGO_TARGET_DIR": str(args.target_dir.resolve() / name)}
    metadata_command = [
        *cargo,
        "metadata",
        "--manifest-path",
        str(source / "Cargo.toml"),
        "--format-version",
        "1",
        "--filter-platform",
        "x86_64-unknown-linux-gnu",
        "--features",
        name + "/toucan-zstd",
        "--config",
        "patch.crates-io.zstd-sys.path="
        + json.dumps(str(stage / "zstd-rs/zstd-safe/zstd-sys")),
    ]
    if args.offline:
        metadata_command += ["--offline"]
    metadata = subprocess.run(
        metadata_command,
        cwd=stage,
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
    )
    (out / "metadata.json").write_text(metadata.stdout)
    (out / "metadata.stderr").write_text(metadata.stderr)
    (out / "metadata.command.json").write_text(
        json.dumps(metadata_command, indent=2) + "\n"
    )
    assert metadata.returncode == 0, metadata.stderr
    if name == "ty":
        data = json.loads(metadata.stdout)
        ids = {p["name"]: p["id"] for p in data["packages"]}
        node = next(
            n for n in data["resolve"]["nodes"] if n["id"] == ids["ty_vendored"]
        )
        edge = next(d for d in node["deps"] if d["pkg"] == ids["zstd-sys"])
        assert {kind["kind"] for kind in edge["dep_kinds"]} == {None, "build"}
    cases = {}
    for mode, features in [("default", []), ("toucan", [name + "/toucan-zstd"])]:
        command = [
            *cargo,
            "tree",
            "--manifest-path",
            str(source / "Cargo.toml"),
            "-p",
            name,
            "--locked",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--format",
            "{p}|{f}",
            "--config",
            "patch.crates-io.zstd-sys.path="
            + json.dumps(str(stage / "zstd-rs/zstd-safe/zstd-sys")),
        ]
        if features:
            command += ["--features", ",".join(features)]
        if args.offline:
            command += ["--offline"]
        result = subprocess.run(
            command, cwd=stage, env=env, capture_output=True, text=True, timeout=120
        )
        (out / (mode + "-tree.command.json")).write_text(
            json.dumps(command, indent=2) + "\n"
        )
        (out / (mode + "-tree.txt")).write_text(result.stdout)
        (out / (mode + "-tree.stderr")).write_text(result.stderr)
        assert result.returncode == 0, result.stderr
        names = {line.split(" v")[0] for line in result.stdout.splitlines()}
        assert ("toucan_bindgen" in names) == (mode == "toucan"), (name, mode)
        assert not {"bindgen", "clang-sys"} & names
        sys = [
            line for line in result.stdout.splitlines() if line.startswith("zstd-sys v")
        ]
        assert sys and all(
            ("toucan" in line.split("|", 1)[1].removesuffix(" (*)").split(","))
            == (mode == "toucan")
            for line in sys
        ), (name, mode, sys)
        cases[mode] = {
            "toucan_active": "toucan_bindgen" in names,
            "bindgen_active": "bindgen" in names,
            "clang_sys_active": "clang-sys" in names,
            "zstd_sys": sorted(set(sys)),
            "command": command,
        }
    original = (stage / (name + "-upstream.lock")).read_text()
    before = tomllib.loads(original)["package"]
    after = tomllib.loads((source / "Cargo.lock").read_text())["package"]
    beforemap = {(x["name"], x["version"]): x for x in before}
    aftermap = {(x["name"], x["version"]): x for x in after}
    missing = sorted(beforemap.keys() - aftermap.keys())
    assert not missing, missing
    changes = []

    def edges(package, packages):
        result = set()
        for entry in package.get("dependencies", []):
            tokens = entry.split()
            candidates = [
                key
                for key in packages
                if key[0] == tokens[0] and (len(tokens) == 1 or key[1] == tokens[1])
            ]
            assert len(candidates) == 1, (entry, candidates)
            result.add(candidates[0])
        return result

    for key, old in beforemap.items():
        new = aftermap[key]
        if key[0] != "zstd-sys":
            assert new.get("source") == old.get("source") and new.get(
                "checksum"
            ) == old.get("checksum"), key
        old_edges = edges(old, beforemap)
        new_edges = edges(new, aftermap)
        assert not old_edges - new_edges, (key, old_edges - new_edges)
        if new_edges - old_edges:
            changes.append(
                {
                    "package": list(key),
                    "added_dependencies": sorted(new_edges - old_edges),
                }
            )
    summary[name] = {
        "cases": cases,
        "original_package_versions_preserved": len(beforemap),
        "original_dependency_edges_preserved": True,
        "added_packages": [list(x) for x in sorted(aftermap.keys() - beforemap.keys())],
        "added_dependency_edges": changes,
    }
    print(name, "graphs validated")
ruff_source = Path(preparation["projects"]["ty"]["source"])
ruff_command = [
    *cargo,
    "tree",
    "--manifest-path",
    str(ruff_source / "Cargo.toml"),
    "-p",
    "ruff",
    "--locked",
    "--target",
    "x86_64-unknown-linux-gnu",
    "--edges",
    "normal,build",
    "--prefix",
    "none",
    "--format",
    "{p}|{f}",
    "--config",
    "patch.crates-io.zstd-sys.path="
    + json.dumps(str(stage / "zstd-rs/zstd-safe/zstd-sys")),
]
if args.offline:
    ruff_command.append("--offline")
ruff = subprocess.run(
    ruff_command, cwd=stage, env=env, capture_output=True, text=True, timeout=120
)
(stage / "evidence/ruff-baseline-tree.txt").write_text(ruff.stdout)
(stage / "evidence/ruff-baseline-tree.stderr").write_text(ruff.stderr)
assert ruff.returncode == 0, ruff.stderr
ruff_names = {line.split(" v")[0] for line in ruff.stdout.splitlines()}
assert not ruff_names & {
    "zstd",
    "zstd-safe",
    "zstd-sys",
    "bindgen",
    "clang-sys",
    "toucan_bindgen",
}
summary["ruff"] = {"default_graph_has_zstd_or_bindgen": False, "command": ruff_command}
print("ruff baseline graph validated")
(stage / "evidence/project-graphs.json").write_text(
    json.dumps(summary, indent=2) + "\n"
)
