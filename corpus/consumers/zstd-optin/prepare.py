#!/usr/bin/env python3
"""Prepare checksummed zstd packages and optional pinned Astral source trees."""

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from git_source import load_report

HERE = Path(__file__).resolve().parent
SOURCE_PIN = "66c87396bbe03b22085339a87b8c350c90be280b"
GIT_PIN = "85bf1ad6dcbc5840ade11bf8798785b6da13260a"
PACKAGES = [
    (
        "zstd",
        "0.13.3",
        "e91ee311a569c327171651566e07972200e76fcfe2242a4fa446149a3881c08a",
        "zstd-rs",
    ),
    (
        "zstd-safe",
        "7.2.4",
        "8f49c4d5f0abb602a93fb8736af2a4f4dd9512e36f7f570d66e65ff867ed3b9d",
        "zstd-rs/zstd-safe",
    ),
    (
        "zstd-sys",
        "2.0.16+zstd.1.5.7",
        "91e19ebc2adc8f83e43039e79776e3fda8ca919132d68a1fed6a5faca2683748",
        "zstd-rs/zstd-safe/zstd-sys",
    ),
]
PROJECTS = {
    "ty": (
        "ruff-e7adf82ff005f3ab3051c363464cf65bf8a6e2f3",
        "babff09f8302fb18bc0ba465983801acccb2035777f8ff43b1da1f25a9c23e5b",
    ),
    "uv": (
        "uv-d28a3ee3d0f7122b0da64b0226d2e173e7d23747",
        "9ef2613de29543b8e1975837ed1483d245db4fefaaa9f79507fc87a0dfc63af0",
    ),
}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def extract(archive, expected, parent):
    assert digest(archive) == expected, f"archive checksum mismatch: {archive}"
    parent.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive) as tar:
        tar.extractall(parent, filter="data")


def apply(directory, patch):
    subprocess.run(
        ["git", "-C", str(directory), "apply", "--check", str(patch)], check=True
    )
    subprocess.run(["git", "-C", str(directory), "apply", str(patch)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument(
        "--crate-cache",
        type=Path,
        required=True,
        help="Cargo registry cache containing the three pinned .crate files",
    )
    parser.add_argument(
        "--toucan-source",
        type=Path,
        help="Immutable source matching source-digests.json (local mode)",
    )
    parser.add_argument("--frontend-mode", choices=["local", "git"], default="local")
    parser.add_argument("--git-source-report", type=Path)
    parser.add_argument("--ruff-archive", type=Path)
    parser.add_argument("--uv-archive", type=Path)
    args = parser.parse_args()
    work = args.work_dir.resolve()
    git_source = None
    if args.frontend_mode == "git":
        if args.git_source_report is None or args.toucan_source is not None:
            parser.error("Git mode requires --git-source-report and no --toucan-source")
        git_source = load_report(args.git_source_report)
        source = Path(git_source["root"])
    else:
        if args.toucan_source is None or args.git_source_report is not None:
            parser.error(
                "local mode requires --toucan-source and no --git-source-report"
            )
        source = args.toucan_source.resolve()
    if work.exists() and any(work.iterdir()):
        parser.error("--work-dir must be empty")
    work.mkdir(parents=True, exist_ok=True)
    expected = json.loads((HERE / "source-digests.json").read_text())
    for relative, value in expected.items():
        assert digest(source / relative) == value, f"Toucan source mismatch: {relative}"
    inputs = []
    for name, version, checksum, relative in PACKAGES:
        matches = list(args.crate_cache.rglob(f"{name}-{version}.crate"))
        matches = [path for path in matches if digest(path) == checksum]
        assert matches, f"missing verified archive: {name}-{version}.crate"
        archive = matches[0]
        scratch = work / "extract" / name
        extract(archive, checksum, scratch)
        package = scratch / f"{name}-{version}"
        destination = work / relative
        shutil.move(str(package), destination)
        # Published .orig manifests retain the upstream nested path dependencies.
        shutil.copyfile(destination / "Cargo.toml.orig", destination / "Cargo.toml")
        inputs.append({"package": name, "version": version, "archive_sha256": checksum})
    patch = HERE / "patches/zstd-rs.patch"
    apply(work / "zstd-rs", patch)
    manifest = work / "zstd-rs/zstd-safe/zstd-sys/Cargo.toml"
    before = manifest.read_text()
    expected_git = f'git = "https://github.com/astral-sh/toucan"\nrev = "{GIT_PIN}"'
    assert before.count(expected_git) == 1
    local = 'version = "=0.0.1"\npath = ' + json.dumps(
        str(source / "crates/toucan_bindgen")
    )
    if args.frontend_mode == "local":
        manifest.write_text(before.replace(expected_git, local))
    # The local substitution is explicit and recorded, never applied to a registry source.
    shutil.copytree(HERE / "fixtures/consumer", work / "consumer")
    shutil.copytree(HERE / "fixtures/dual-graph", work / "dual-graph")
    for fixture in ("consumer", "dual-graph"):
        (work / fixture / "Cargo.toml.in").rename(work / fixture / "Cargo.toml")
    projects = {}
    for name, archive in [("ty", args.ruff_archive), ("uv", args.uv_archive)]:
        if archive is None:
            continue
        dirname, checksum = PROJECTS[name]
        parent = work / "projects" / name
        extract(archive, checksum, parent)
        project = parent / dirname
        original_manifests = {}
        for path in project.rglob("Cargo.toml"):
            original_manifests[str(path.relative_to(project))] = digest(path)
        apply(project, HERE / "patches" / f"{name}.patch")
        shutil.copyfile(project / "Cargo.lock", work / (name + "-upstream.lock"))
        projects[name] = {
            "source": str(project),
            "archive_sha256": checksum,
            "original_manifest_sha256": original_manifests,
        }
    provenance = {
        "frontend_mode": args.frontend_mode,
        "git_source": git_source,
        "frontend_source_revision": SOURCE_PIN,
        "integration_git_revision": GIT_PIN,
        "toucan_source": str(source),
        "verified_source_files": len(expected),
        "source_inventory_sha256": digest(HERE / "source-digests.json"),
        "inputs": inputs,
        "patch_sha256": {
            p.name: digest(p) for p in sorted((HERE / "patches").glob("*.patch"))
        },
        "local_override": {
            "reviewed_git": expected_git,
            "smoke_dependency": local,
            "manifest_sha256": digest(manifest),
        }
        if args.frontend_mode == "local"
        else None,
        "projects": projects,
    }
    (work / "preparation.json").write_text(json.dumps(provenance, indent=2) + "\n")
    print(work)


if __name__ == "__main__":
    main()
