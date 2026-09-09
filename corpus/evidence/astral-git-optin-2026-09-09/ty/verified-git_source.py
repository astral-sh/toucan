#!/usr/bin/env python3
"""Fetch and audit the exact Git source used by the opt-in consumer trial."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shlex
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
GIT_URL = "https://github.com/astral-sh/toucan"
GIT_PIN = "85bf1ad6dcbc5840ade11bf8798785b6da13260a"
GIT_SOURCE = f"git+{GIT_URL}?rev={GIT_PIN}#{GIT_PIN}"
PACKAGES = frozenset(
    [
        "toucan",
        "toucan_bindgen",
        "toucan_bindings",
        "toucan_layout",
        "toucan_parser",
        "toucan_preprocessor",
        "toucan_semantic",
        "toucan_source",
        "toucan_target",
    ]
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inventory(root: Path) -> dict[str, str]:
    """Include every file in the frozen build-input scope, rejecting symlinks."""
    paths = [root / "Cargo.toml", root / "Cargo.lock"]
    for path in (root / "crates").rglob("*"):
        require(not path.is_symlink(), f"symlink in frontend source: {path}")
        if not path.is_dir():
            paths.append(path)
    result = {}
    for path in sorted(paths):
        require(not path.is_symlink(), f"symlink in frontend source: {path}")
        result[path.relative_to(root).as_posix()] = digest(path)
    return result


def verify_checkout(root: Path) -> dict:
    root = root.resolve(strict=True)
    head = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    require(
        head == GIT_PIN, f"fetched frontend HEAD differs from the trial pin: {head}"
    )
    top = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "--show-toplevel"], text=True
    ).strip()
    require(Path(top).resolve() == root, "frontend source is not a Git checkout root")
    expected = json.loads((HERE / "source-digests.json").read_text())
    actual = inventory(root)
    require(
        actual == expected, "fetched frontend differs from the exact source inventory"
    )
    return {
        "root": str(root),
        "head": head,
        "source": GIT_SOURCE,
        "verified_source_files": len(actual),
        "source_inventory_sha256": digest(HERE / "source-digests.json"),
    }


def verify_packages(metadata: dict, root: Path | None = None) -> dict:
    """Reject source-ID camouflage, escaped manifests and mixed Git checkouts."""
    packages = [
        p
        for p in metadata["packages"]
        if p["name"] == "toucan" or p["name"].startswith("toucan_")
    ]
    require(
        len(packages) == len(PACKAGES) and {p["name"] for p in packages} == PACKAGES,
        "expected exactly the nine frontend packages",
    )
    bindgen = next(p for p in packages if p["name"] == "toucan_bindgen")
    discovered = Path(bindgen["manifest_path"]).resolve(strict=True).parents[2]
    if root is not None:
        require(discovered == root.resolve(strict=True), "frontend checkout changed")
    report = verify_checkout(discovered)
    entries = {}
    for package in packages:
        name = package["name"]
        manifest = Path(package["manifest_path"]).resolve(strict=True)
        require(package["source"] == GIT_SOURCE, f"wrong Git source for {name}")
        require(package["version"] == "0.0.1", f"wrong version for {name}")
        require(
            manifest == discovered / "crates" / name / "Cargo.toml",
            f"frontend manifest escapes the fetched checkout: {name}",
        )
        targets = []
        for target in package["targets"]:
            path = Path(target["src_path"]).resolve(strict=True)
            require(
                path.is_relative_to(manifest.parent), f"escaped source target: {name}"
            )
            targets.append(str(path))
        entries[package["id"]] = {
            "name": name,
            "manifest_path": str(manifest),
            "target_sources": sorted(targets),
        }
    report["packages"] = entries
    return report


def load_report(path: Path) -> dict:
    report = json.loads(path.read_text())
    current = verify_checkout(Path(report["root"]))
    for key, value in current.items():
        require(report.get(key) == value, f"stale Git source report: {key}")
    require(
        len(report["packages"]) == len(PACKAGES)
        and {p["name"] for p in report["packages"].values()} == PACKAGES,
        "invalid frontend package report",
    )
    return report


def verify_artifacts(log: Path, report: dict) -> list[dict]:
    """Link each built frontend library back to the audited Cargo package."""
    expected = report["packages"]
    targets = {source for p in expected.values() for source in p["target_sources"]}
    rows = []
    seen = set()
    for line in log.read_text().splitlines():
        row = json.loads(line)
        if row.get("reason") != "compiler-artifact":
            continue
        package_id = row["package_id"]
        manifest = Path(row["manifest_path"]).resolve(strict=True)
        source = Path(row["target"]["src_path"]).resolve(strict=True)
        frontend = (
            package_id in expected
            or manifest.parent.name in PACKAGES
            or str(source) in targets
            or row["target"]["name"] in PACKAGES
        )
        if not frontend:
            continue
        require(
            package_id in expected, "built frontend has an unaudited package identity"
        )
        package = expected[package_id]
        require(
            str(manifest) == package["manifest_path"],
            "built frontend has a different manifest",
        )
        require(
            str(source) in package["target_sources"],
            "built frontend has a different source",
        )
        if "lib" not in row["target"]["kind"]:
            continue
        seen.add(package_id)
        files = {p: digest(Path(p)) for p in row["filenames"]}
        require(files, "frontend library artifact has no files")
        rows.append(
            {
                "package_id": package_id,
                "manifest_path": str(manifest),
                "src_path": str(source),
                "features": row["features"],
                "fresh": row["fresh"],
                "files": files,
            }
        )
    require(seen == set(expected), "build did not report all nine frontend libraries")
    verify_checkout(Path(report["root"]))
    return rows


def credential_request(operation: str, request: str) -> None:
    """Answer one repository's HTTPS request without persisting credentials."""
    if operation != "get":
        return
    fields = dict(line.split("=", 1) for line in request.splitlines() if "=" in line)
    allowed = (
        fields.get("protocol") == "https"
        and fields.get("host") == "github.com"
        and fields.get("path") in {"astral-sh/toucan", "astral-sh/toucan.git"}
    )
    if not allowed:
        print("quit=true")
        return
    token = os.environ.get("TOUCAN_GIT_TOKEN")
    if token:
        # stdout is the private pipe to Git, never a retained command output.
        sys.stdout.write(f"username=x-access-token\npassword={token}\n\n")
    else:
        helper = os.environ.get("TOUCAN_GIT_GH_AUTO")
        require(
            helper == "/home/dev-user/.local/bin/gh-auto",
            "no scoped Git authentication configured",
        )
        subprocess.run(
            [helper, "auth", "git-credential", "get"],
            input=request,
            text=True,
            check=True,
            env=os.environ | {"GH_AUTO_ACCOUNT": "oss"},
        )


def main() -> None:
    if len(sys.argv) == 3 and sys.argv[1] == "credentials":
        credential_request(sys.argv[2], sys.stdin.read())
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rust-toolchain")
    parser.add_argument(
        "--gh-auto",
        action="store_true",
        help="Use the devbox's scoped OSS credential helper",
    )
    args = parser.parse_args()
    cache = args.cache.resolve()
    require(not cache.exists(), "use a fresh Git prefetch directory")
    cache.mkdir(parents=True)
    (cache / "Cargo.toml").write_text(
        '[package]\nname="toucan-optin-git-fetch"\nversion="0.0.0"\nedition="2024"\n'
        "[workspace]\n[dependencies]\ntoucan_bindgen={git="
        + json.dumps(GIT_URL)
        + ",rev="
        + json.dumps(GIT_PIN)
        + "}\n"
    )
    (cache / "src").mkdir()
    (cache / "src/lib.rs").write_text("// Fetch only; this package is never built.\n")
    env = os.environ.copy()
    require(
        not (args.gh_auto and env.get("TOUCAN_GIT_TOKEN")),
        "select one authentication source",
    )
    if args.gh_auto:
        env["TOUCAN_GIT_GH_AUTO"] = "/home/dev-user/.local/bin/gh-auto"
    env.update(
        {
            "CARGO_NET_GIT_FETCH_WITH_CLI": "true",
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_CONFIG_COUNT": "3",
            "GIT_CONFIG_KEY_0": "credential.helper",
            "GIT_CONFIG_VALUE_0": "",
            "GIT_CONFIG_KEY_1": "credential.useHttpPath",
            "GIT_CONFIG_VALUE_1": "true",
            "GIT_CONFIG_KEY_2": "credential.helper",
            "GIT_CONFIG_VALUE_2": "!"
            + shlex.join(
                [sys.executable, str(Path(__file__).resolve()), "credentials"]
            ),
        }
    )
    for name in ["GIT_TRACE", "GIT_TRACE_CURL", "GIT_CURL_VERBOSE", "GIT_TRACE_PACKET"]:
        env.pop(name, None)
    cargo = ["cargo"] + (["+" + args.rust_toolchain] if args.rust_toolchain else [])
    command = cargo + [
        "metadata",
        "--format-version=1",
        "--manifest-path",
        str(cache / "Cargo.toml"),
    ]
    completed = subprocess.run(
        command, env=env, text=True, capture_output=True, timeout=600, check=False
    )
    require(completed.returncode == 0, "Git prefetch failed: " + completed.stderr)
    metadata = json.loads(completed.stdout)
    report = verify_packages(metadata)
    report.update(
        command=command,
        metadata_sha256=hashlib.sha256(completed.stdout.encode()).hexdigest(),
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Verified {len(report['packages'])} Git packages at {report['head']}")


if __name__ == "__main__":
    main()
