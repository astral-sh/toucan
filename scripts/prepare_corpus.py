#!/usr/bin/env python3
"""Fetch pinned source archives, verify them, and build native static libraries."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import platform
import shutil
import subprocess
import tarfile
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "corpus" / "manifest.json"


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(command: list[str], directory: Path, log: Path) -> dict:
    started = time.monotonic()
    with log.open("wb") as output:
        result = subprocess.run(
            command, cwd=directory, stdout=output, stderr=subprocess.STDOUT, check=False
        )
    entry = {
        "command": command,
        "cwd": str(directory),
        "seconds": time.monotonic() - started,
        "exit_code": result.returncode,
        "log": str(log),
    }
    if result.returncode:
        tail = "\n".join(log.read_text(errors="replace").splitlines()[-30:])
        raise RuntimeError(f"command failed: {command!r}\n{tail}\nFull log: {log}")
    return entry


def extract(archive: Path, destination: Path, expected_root: str) -> None:
    """Validate member paths and use Python's data filter before atomic placement."""
    marker = destination / ".toucan-archive-sha256"
    expected_hash = digest(archive)
    if destination.exists():
        if marker.read_text().strip() != expected_hash:
            raise RuntimeError(
                f"existing source tree has another archive hash: {destination}"
            )
        return
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix=".extract-", dir=destination.parent
    ) as temporary:
        staging = Path(temporary)
        with tarfile.open(archive, "r:gz") as bundle:
            for member in bundle.getmembers():
                parts = Path(member.name).parts
                if (
                    not parts
                    or parts[0] != expected_root
                    or ".." in parts
                    or member.name.startswith("/")
                ):
                    raise RuntimeError(f"unexpected archive member: {member.name}")
                if not (
                    member.isfile()
                    or member.isdir()
                    or member.issym()
                    or member.islnk()
                ):
                    raise RuntimeError(f"unsupported archive member: {member.name}")
            bundle.extractall(staging, filter="data")
        (staging / expected_root).rename(destination)
        marker.write_text(expected_hash + "\n")


def build(project: dict, args: argparse.Namespace) -> dict:
    name = project["name"]
    archive = args.cache / project["archive"]
    if not archive.exists():
        if args.offline:
            raise RuntimeError(f"missing archive in offline mode: {archive}")
        temporary = archive.with_suffix(".download")
        try:
            with temporary.open("wb") as output:
                subprocess.run(
                    [
                        args.gh,
                        "api",
                        f"repos/{project['repository']}/tarball/{project['archive_ref']}",
                    ],
                    cwd=ROOT,
                    stdout=output,
                    check=True,
                )
            if digest(temporary) != project["sha256"]:
                raise RuntimeError(
                    f"SHA256 mismatch for downloaded {name}; expected {project['sha256']}"
                )
            temporary.rename(archive)
        finally:
            temporary.unlink(missing_ok=True)
    actual_hash = digest(archive)
    if actual_hash != project["sha256"]:
        raise RuntimeError(f"SHA256 mismatch for {archive}: {actual_hash}")
    source = args.cache / "sources" / f"{name}-{project['version']}"
    build_dir = args.cache / "build" / name
    log_dir = args.cache / "logs" / name
    extract(archive, source, project["archive_root"])
    build_dir.mkdir(parents=True, exist_ok=True)
    log_dir.mkdir(parents=True, exist_ok=True)
    commands = []

    def execute(command: list[str], directory: Path = build_dir) -> None:
        commands.append(run(command, directory, log_dir / f"{len(commands):02}.log"))

    cmake = [
        "cmake",
        "-S",
        str(source),
        "-B",
        str(build_dir),
        "-DCMAKE_BUILD_TYPE=Release",
        f"-DCMAKE_C_COMPILER={args.cc}",
    ]
    if name == "zlib":
        execute(cmake + ["-DBUILD_SHARED_LIBS=OFF", "-DZLIB_BUILD_EXAMPLES=OFF"])
        execute(
            [
                "cmake",
                "--build",
                str(build_dir),
                "--target",
                "zlibstatic",
                "--parallel",
                str(args.jobs),
            ]
        )
        header = source / "zlib.h"
        includes = [build_dir, source]
        libraries = [build_dir / "libz.a"]
    elif name == "zstd":
        execute(
            [
                "make",
                "-C",
                str(source / "lib"),
                f"-j{args.jobs}",
                "libzstd.a",
                f"CC={args.cc}",
            ]
        )
        header = source / "lib" / "zstd.h"
        includes = [source / "lib"]
        libraries = [source / "lib" / "libzstd.a"]
    elif name == "sqlite":
        execute(
            [
                str(source / "configure"),
                "--disable-shared",
                "--enable-static",
                "--disable-tcl",
                f"CC={args.cc}",
            ]
        )
        execute(["make", f"-j{args.jobs}", "sqlite3.c", "sqlite3.h"])
        execute(
            [
                args.cc,
                "-O2",
                "-DSQLITE_THREADSAFE=0",
                "-c",
                "sqlite3.c",
                "-o",
                "sqlite3.o",
            ]
        )
        execute(["ar", "rcs", "libsqlite3.a", "sqlite3.o"])
        header = build_dir / "sqlite3.h"
        includes = [build_dir]
        libraries = [build_dir / "libsqlite3.a"]
    elif name == "libgit2":
        execute(
            cmake
            + [
                "-DBUILD_SHARED_LIBS=OFF",
                "-DBUILD_TESTS=OFF",
                "-DBUILD_CLI=OFF",
                "-DUSE_SSH=OFF",
                "-DUSE_HTTPS=OFF",
                "-DUSE_NTLMCLIENT=OFF",
                "-DUSE_BUNDLED_ZLIB=ON",
                "-DREGEX_BACKEND=builtin",
                "-DUSE_ICONV=OFF",
            ]
        )
        execute(["cmake", "--build", str(build_dir), "--parallel", str(args.jobs)])
        header = source / "include" / "git2.h"
        includes = [source / "include", build_dir / "include"]
        libraries = [build_dir / "libgit2.a"]
    else:
        raise RuntimeError(f"unknown project: {name}")
    for path in [header, *libraries]:
        if not path.is_file():
            raise RuntimeError(f"build did not produce {path}")
    result = {
        **project,
        "source": str(source),
        "build": str(build_dir),
        "header": str(header),
        "include_dirs": [str(path) for path in includes],
        "libraries": [str(path) for path in libraries],
        "library_sha256": {str(path): digest(path) for path in libraries},
        "commands": commands,
    }
    print(f"Prepared {name} {project['version']}", flush=True)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=ROOT / "corpus" / "cache")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--jobs", type=int, default=min(os.cpu_count() or 2, 8))
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--cc", default=os.environ.get("CC", "cc"))
    default_gh = shutil.which("gh-auto")
    if default_gh is None:
        local_router = Path.home() / ".local" / "bin" / "gh-auto"
        default_gh = (
            str(local_router) if local_router.is_file() else shutil.which("gh") or "gh"
        )
    parser.add_argument("--gh", default=default_gh)
    parser.add_argument(
        "--project", action="append", choices=["zlib", "sqlite", "zstd", "libgit2"]
    )
    args = parser.parse_args()
    if args.jobs < 1 or args.workers < 1:
        parser.error("--jobs and --workers must be positive")
    args.cache = args.cache.resolve()
    args.cache.mkdir(parents=True, exist_ok=True)
    projects = json.loads(MANIFEST.read_text())["projects"]
    if args.project:
        projects = [project for project in projects if project["name"] in args.project]
    prepared = []
    failures = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        futures = {
            pool.submit(build, project, args): project["name"] for project in projects
        }
        for future in concurrent.futures.as_completed(futures):
            try:
                prepared.append(future.result())
            except (
                OSError,
                RuntimeError,
                ValueError,
                subprocess.SubprocessError,
                tarfile.TarError,
            ) as error:
                failures.append({"project": futures[future], "error": str(error)})
                print(f"FAILED {futures[future]}: {error}", flush=True)
    metadata = {
        "schema_version": 1,
        "host": platform.platform(),
        "compiler": subprocess.check_output(
            [args.cc, "--version"], text=True
        ).splitlines()[0],
        "projects": sorted(prepared, key=lambda project: project["name"]),
        "failures": failures,
    }
    output = args.cache / "prepared.json"
    output.write_text(json.dumps(metadata, indent=2) + "\n")
    print(output, flush=True)
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
