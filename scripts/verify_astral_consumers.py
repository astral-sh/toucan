#!/usr/bin/env python3
"""Compare pinned ty/uv binaries built with upstream and Toucan zstd bindings."""

from __future__ import annotations

import argparse
import base64
import contextlib
import ctypes
import ctypes.util
import datetime
import hashlib
import http.server
import io
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import threading
import zipfile
from pathlib import Path


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def run(
    command: list[str],
    cwd: Path,
    *,
    expected: int = 0,
    timeout: int = 120,
    env: dict | None = None,
) -> dict:
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    evidence = {
        "command": command,
        "cwd": str(cwd),
        "exit_code": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }
    if result.returncode != expected:
        raise RuntimeError(f"unexpected exit status: {json.dumps(evidence, indent=2)}")
    return evidence


TY_CASES = {
    "valid": """from pathlib import Path
from datetime import datetime
from collections.abc import Iterable
import json

def names(paths: Iterable[Path]) -> list[str]:
    return [path.name for path in paths]

payload: str = json.dumps({"name": Path("/tmp/example").name})
stamp: datetime = datetime.fromisoformat("2026-09-08")
""",
    "invalid": """from pathlib import Path
from datetime import datetime

name: int = Path("/tmp/example").name
stamp: str = datetime.fromisoformat("2026-09-08")
""",
}


def ty_runtime(binary: Path, directory: Path) -> dict:
    directory.mkdir(parents=True, exist_ok=True)
    result = {"binary_sha256": digest(binary), "cases": {}}
    for name, source in TY_CASES.items():
        path = directory / f"{name}.py"
        path.write_text(source)
        case = run(
            [
                str(binary),
                "check",
                "--no-progress",
                "--color",
                "never",
                "--output-format",
                "concise",
                str(path),
            ],
            directory,
            expected=0 if name == "valid" else 1,
        )
        case["source_sha256"] = digest(path)
        result["cases"][name] = case
    assert result["cases"]["valid"]["stdout"] == "All checks passed!\n"
    assert result["cases"]["invalid"]["stdout"] == (
        "invalid.py:4:13: error[invalid-assignment] Object of type `str` is not assignable to `int`\n"
        "invalid.py:5:14: error[invalid-assignment] Object of type `datetime` is not assignable to `str`\nFound 2 diagnostics\n"
    )
    assert all(not case["stderr"] for case in result["cases"].values())
    return result


def wheel() -> tuple[bytes, dict[str, bytes]]:
    """Produce a deterministic, dependency-free wheel with verified RECORD hashes."""
    files = {
        "toucan_consumer_probe/__init__.py": b'VALUE = "toucan-zstd-workspace-probe"\n',
        "toucan_consumer_probe/data.bin": bytes(range(256)) * 257,
        "toucan_consumer_probe-1.0.0.dist-info/METADATA": b"Metadata-Version: 2.1\nName: toucan-consumer-probe\nVersion: 1.0.0\n",
        "toucan_consumer_probe-1.0.0.dist-info/WHEEL": b"Wheel-Version: 1.0\nGenerator: toucan-conformance\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    }
    records = []
    for name, data in files.items():
        value = (
            base64.urlsafe_b64encode(hashlib.sha256(data).digest())
            .rstrip(b"=")
            .decode()
        )
        records.append(f"{name},sha256={value},{len(data)}\n")
    record = "toucan_consumer_probe-1.0.0.dist-info/RECORD"
    files[record] = ("".join(records) + f"{record},,\n").encode()
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_STORED) as archive:
        for name, data in files.items():
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.external_attr = 0o100644 << 16
            archive.writestr(info, data)
    return output.getvalue(), files


def zstd_compress(data: bytes) -> tuple[bytes, dict]:
    """Use a local libzstd as the fixture encoder; the tested consumer decodes it."""
    name = ctypes.util.find_library("zstd")
    if not name:
        raise RuntimeError("install a native libzstd to encode the HTTP fixture")
    library = ctypes.CDLL(name)
    library.ZSTD_compressBound.argtypes = [ctypes.c_size_t]
    library.ZSTD_compressBound.restype = ctypes.c_size_t
    library.ZSTD_compress.argtypes = [
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.c_void_p,
        ctypes.c_size_t,
        ctypes.c_int,
    ]
    library.ZSTD_compress.restype = ctypes.c_size_t
    library.ZSTD_isError.argtypes = [ctypes.c_size_t]
    library.ZSTD_isError.restype = ctypes.c_uint
    library.ZSTD_versionString.argtypes = []
    library.ZSTD_versionString.restype = ctypes.c_char_p
    bound = library.ZSTD_compressBound(len(data))
    target = ctypes.create_string_buffer(bound)
    source = ctypes.create_string_buffer(data)
    written = library.ZSTD_compress(target, bound, source, len(data), 3)
    if library.ZSTD_isError(written) or written > bound:
        raise RuntimeError("libzstd failed to compress the deterministic wheel")
    return target.raw[:written], {
        "library": name,
        "version": library.ZSTD_versionString().decode(),
    }


@contextlib.contextmanager
def wheel_server(data: bytes):
    requests: list[dict] = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append(
                {"method": "GET", "path": self.path, "headers": dict(self.headers)}
            )
            if self.path != "/toucan_consumer_probe-1.0.0-py3-none-any.whl":
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Encoding", "zstd")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(data)

        def do_HEAD(self):
            requests.append(
                {"method": "HEAD", "path": self.path, "headers": dict(self.headers)}
            )
            self.send_response(200)
            self.send_header("Content-Encoding", "zstd")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()

        def log_message(self, *_args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield (
            f"http://127.0.0.1:{server.server_port}/toucan_consumer_probe-1.0.0-py3-none-any.whl",
            requests,
        )
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def uv_runtime(
    binary: Path,
    directory: Path,
    url: str,
    expected_files: dict[str, bytes],
    python: str,
    *,
    expected_exit: int = 0,
) -> dict:
    directory.mkdir(parents=True, exist_ok=True)
    installed = directory / "installed"
    if installed.exists():
        shutil.rmtree(installed)
    # Do not inherit registry/authentication configuration or write to shared caches.
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("UV_", "PIP_"))
    }
    environment.update(
        {
            "UV_NO_CONFIG": "1",
            "UV_NO_CACHE": "1",
            "UV_PYTHON_DOWNLOADS": "never",
            "UV_HTTP_RETRIES": "0",
            "PYTHONDONTWRITEBYTECODE": "1",
            "NO_PROXY": "127.0.0.1,localhost",
        }
    )
    result = run(
        [
            str(binary),
            "--no-progress",
            "--color",
            "never",
            "pip",
            "install",
            "--no-deps",
            "--no-index",
            "--link-mode",
            "copy",
            "--python",
            python,
            "--target",
            str(installed),
            url,
        ],
        directory,
        env=environment,
        expected=expected_exit,
    )
    result["binary_sha256"] = digest(binary)
    if expected_exit:
        result["fixture_installed"] = (installed / "toucan_consumer_probe").exists()
        if result["fixture_installed"]:
            raise RuntimeError("uv installed a package from a truncated zstd response")
        return result
    checked = {}
    for name, data in expected_files.items():
        if name.endswith("/RECORD"):
            continue  # Installers add their own metadata and update RECORD.
        actual = installed / name
        if actual.read_bytes() != data:
            raise RuntimeError(f"installed wheel bytes differ: {actual}")
        checked[name] = digest(actual)
    result["installed_fixture_sha256"] = checked
    result["import"] = run(
        [
            python,
            "-I",
            "-c",
            "import sys; sys.path.insert(0, sys.argv[1]); import toucan_consumer_probe as p; print(p.VALUE)",
            str(installed),
        ],
        directory,
        env=environment,
    )
    assert result["import"]["stdout"] == "toucan-zstd-workspace-probe\n"
    return result


def source_inventory(directory: Path) -> dict[str, str]:
    result = {}
    for path in sorted(directory.rglob("*")):
        name = path.relative_to(directory).as_posix()
        if path.is_symlink():
            result[name] = "symlink:" + os.readlink(path)
        elif path.is_file():
            result[name] = digest(path)
    return result


def check_archive_source(archive: Path, source: Path) -> dict:
    """Check all extracted inputs against the archive, allowing only lock updates."""
    import tarfile

    expected = {}
    normalized_links = {}
    original_lock = None
    with tarfile.open(archive, "r:gz") as bundle:
        for member in bundle.getmembers():
            parts = Path(member.name).parts
            if (
                not parts
                or parts[0] != source.name
                or ".." in parts
                or member.name.startswith("/")
            ):
                raise RuntimeError(f"unexpected source archive path: {member.name}")
            name = Path(*parts[1:]).as_posix()
            if member.issym():
                filtered = tarfile.data_filter(member, str(source.parent))
                expected[name] = "symlink:" + filtered.linkname
                if member.linkname != filtered.linkname:
                    normalized_links[name] = {
                        "archive": member.linkname,
                        "extracted": filtered.linkname,
                    }
            elif member.isfile():
                contents = bundle.extractfile(member).read()
                expected[name] = hashlib.sha256(contents).hexdigest()
                if name == "Cargo.lock":
                    original_lock = contents
            elif not member.isdir():
                raise RuntimeError(f"unsupported source archive member: {member.name}")
    actual = source_inventory(source)
    changed = sorted(
        name
        for name in expected.keys() | actual.keys()
        if expected.get(name) != actual.get(name)
    )
    if set(changed) - {"Cargo.lock"}:
        raise RuntimeError(f"upstream project sources changed: {changed}")
    if original_lock is None:
        raise RuntimeError("pinned archive has no Cargo.lock")
    original = dict(expected)
    original.pop("Cargo.lock")
    return {
        "source_files": len(expected),
        "archive_link_normalizations": normalized_links,
        "source_sha256_without_lock": hashlib.sha256(
            json.dumps(original, sort_keys=True).encode()
        ).hexdigest(),
        "changed_files": changed,
        "archive_lock": original_lock,
    }


def check_locks(upstream: Path, generated: Path) -> dict:
    import tomllib

    original = tomllib.loads(upstream.read_text())
    patched = tomllib.loads(generated.read_text())
    selected = [p for p in original["package"] if p["name"] == "zstd-sys"]
    if len(selected) != 1 or selected[0]["version"] != "2.0.16+zstd.1.5.7":
        raise RuntimeError("unexpected locked zstd-sys version")
    source = selected[0].pop("source")
    checksum = selected[0].pop("checksum")
    if original != patched:
        raise RuntimeError("lock changes extend beyond zstd-sys source and checksum")
    return {
        "upstream_sha256": digest(upstream),
        "generated_sha256": digest(generated),
        "removed_registry_source": source,
        "removed_registry_checksum": checksum,
        "other_changes": [],
    }


def patch_zstd_lockfile(lockfile: Path) -> None:
    """Replace only zstd-sys's source identity, then let --locked verify resolution."""
    import re

    import tomllib

    parts = re.split(r"(?m)(?=^\[\[package\]\]$)", lockfile.read_text())
    found = 0
    for index, part in enumerate(parts):
        if not part.startswith("[[package]]"):
            continue
        package = tomllib.loads(part)["package"][0]
        if package["name"] != "zstd-sys":
            continue
        if (
            package.get("source")
            != "registry+https://github.com/rust-lang/crates.io-index"
        ):
            raise RuntimeError("upstream zstd-sys must come from crates.io")
        if "checksum" not in package:
            raise RuntimeError("upstream zstd-sys has no checksum")
        parts[index] = "".join(
            line
            for line in part.splitlines(keepends=True)
            if not line.startswith(("source = ", "checksum = "))
        )
        found += 1
    if found != 1:
        raise RuntimeError(f"expected one zstd-sys lock entry, found {found}")
    lockfile.write_text("".join(parts))


def check_zstd_source(source: Path, report: Path) -> dict:
    evidence = json.loads(report.read_text())
    expected_changes = ["src/bindings_zdict.rs", "src/bindings_zstd.rs"]
    if (
        evidence["status"] != "passed"
        or evidence["profile"] != "default"
        or evidence["changed_upstream_files"] != expected_changes
    ):
        raise RuntimeError("use a passing default-profile verify_zstd_consumer report")
    actual = source_inventory(source)
    if actual != evidence["tested_source_sha256"]:
        raise RuntimeError(
            "prepared zstd-sys differs from its verified source inventory"
        )
    changed = sorted(
        name
        for name in actual.keys() | evidence["upstream_source_sha256"].keys()
        if actual.get(name) != evidence["upstream_source_sha256"].get(name)
    )
    if changed != expected_changes:
        raise RuntimeError(f"unexpected zstd-sys changes: {changed}")
    return {
        "report": str(report),
        "report_sha256": digest(report),
        "toucan_sha256": evidence["toucan_sha256"],
        "target": evidence["target"],
        "changed_upstream_files": changed,
        "generated_bindings": evidence["generated_bindings"],
    }


def binding_artifacts(log: Path, source: Path | None, output: Path) -> list[dict]:
    """Use Cargo's selected artifacts to locate dep-info, avoiding stale cache files."""
    result = []
    for line in log.read_text().splitlines():
        artifact = json.loads(line)
        if (
            artifact.get("reason") != "compiler-artifact"
            or artifact["target"]["name"] != "zstd_sys"
        ):
            continue
        crate = Path(artifact["manifest_path"]).parent
        if source is not None and crate.resolve() != source.resolve():
            raise RuntimeError("Cargo compiled another zstd-sys source directory")
        if source is None and not artifact["package_id"].startswith("registry+"):
            raise RuntimeError("upstream build did not use registry zstd-sys")
        if set(artifact["features"]) & {
            "bindgen",
            "pkg-config",
            "experimental",
            "seekable",
        }:
            raise RuntimeError("unexpected zstd-sys feature selection")
        active = ["src/bindings_zstd.rs"]
        if "zdict_builder" in artifact["features"]:
            active.append("src/bindings_zdict.rs")
        rlib = next(Path(p) for p in artifact["filenames"] if p.endswith(".rlib"))
        dep = rlib.with_name(rlib.stem.removeprefix("lib") + ".d")
        text = dep.read_text().replace("\\ ", " ")
        if not all(str(crate / name) in text for name in active):
            raise RuntimeError(f"selected binding input absent from {dep}")
        output.mkdir(parents=True, exist_ok=True)
        retained = output / dep.name
        shutil.copyfile(dep, retained)
        result.append(
            {
                "package_id": artifact["package_id"],
                "manifest_path": str(crate / "Cargo.toml"),
                "features": artifact["features"],
                "profile": artifact["profile"],
                "dep_info": str(retained),
                "dep_info_sha256": digest(retained),
                "active_bindings": {name: digest(crate / name) for name in active},
            }
        )
    if not result:
        raise RuntimeError(f"no selected zstd-sys artifact in {log}")
    return result


def library_test_executable(log: Path, package: str) -> Path:
    """Select the requested library test executable from this Cargo invocation."""
    executables = []
    for line in log.read_text().splitlines():
        artifact = json.loads(line)
        if (
            artifact.get("reason") == "compiler-artifact"
            and artifact["target"]["name"] == package.replace("-", "_")
            and "lib" in artifact["target"]["kind"]
            and artifact["profile"]["test"]
            and artifact["executable"] is not None
        ):
            executables.append(Path(artifact["executable"]))
    if len(executables) != 1:
        raise RuntimeError(f"expected one {package} library test executable in {log}")
    return executables[0]


def library_test_results(stdout: str) -> dict:
    """Require actual, unfiltered test execution and retain named test results."""
    summaries = re.findall(
        r"^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; "
        r"(\d+) measured; (\d+) filtered out;",
        stdout,
        re.MULTILINE,
    )
    if len(summaries) != 1:
        raise RuntimeError("missing or ambiguous successful library test summary")
    passed, failed, ignored, measured, filtered = map(int, summaries[0])
    names = re.findall(
        r"^test (\S+) \.\.\. (ok|ignored)(?:\b.*)$", stdout, re.MULTILINE
    )
    if (
        passed == 0
        or failed
        or measured
        or filtered
        or len(names) != passed + ignored
        or len({name for name, _ in names}) != len(names)
        or sum(result == "ok" for _, result in names) != passed
    ):
        raise RuntimeError(
            "library tests did not execute the complete selected test suite"
        )
    return {"passed": passed, "ignored": ignored, "tests": dict(sorted(names))}


def prepare_project(project: dict, cache: Path, offline: bool) -> tuple[Path, Path]:
    import tarfile
    import urllib.request

    repository = project["repository"].split("/")[1]
    archive = cache / f"{repository}.tar.gz"
    source = cache / f"{repository}-{project['revision']}"
    if not archive.exists():
        if offline:
            raise RuntimeError(f"missing cached archive: {archive}")
        url = f"https://codeload.github.com/{project['repository']}/tar.gz/{project['revision']}"
        with (
            urllib.request.urlopen(url, timeout=120) as response,
            archive.with_suffix(".download").open("wb") as destination,
        ):
            shutil.copyfileobj(response, destination)
        archive.with_suffix(".download").rename(archive)
    if digest(archive) != project["archive_sha256"]:
        raise RuntimeError(f"archive checksum mismatch: {archive}")
    if not source.exists():
        with tarfile.open(archive, "r:gz") as bundle:
            for member in bundle.getmembers():
                parts = Path(member.name).parts
                if (
                    not parts
                    or parts[0] != source.name
                    or ".." in parts
                    or member.name.startswith("/")
                ):
                    raise RuntimeError(f"invalid archive member: {member.name}")
            bundle.extractall(cache, filter="data")
    return archive, source


def build_project(name: str, project: dict, args: argparse.Namespace) -> dict:
    archive, source = prepare_project(project, args.cache, args.offline)
    before = check_archive_source(archive, source)
    original_lock = before.pop("archive_lock")
    # The source tree is dedicated to this runner, and all other files were
    # checked above. Restore only the archive lock before the upstream build.
    (source / "Cargo.lock").write_bytes(original_lock)
    output = args.output / name
    output.mkdir(parents=True, exist_ok=True)
    target = args.cache / f"{project['repository'].split('/')[1]}-target"
    environment = os.environ | {
        "CARGO_TARGET_DIR": str(target),
        "RUSTUP_TOOLCHAIN": args.rust_toolchain,
    }
    commands = []

    def cargo(arguments: list[str], label: str):
        command = ["cargo", *arguments]
        stdout, stderr = output / f"{label}.jsonl", output / f"{label}.stderr"
        with stdout.open("w") as out, stderr.open("w") as err:
            completed = subprocess.run(
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
                    "CARGO_TARGET_DIR": str(target),
                    "RUSTUP_TOOLCHAIN": args.rust_toolchain,
                },
                "exit_code": completed.returncode,
                "stdout": str(stdout),
                "stderr": str(stderr),
            }
        )
        if completed.returncode:
            raise RuntimeError(f"build step failed; see {stderr}")
        return stdout

    def library_tests(label: str, config: list[str]) -> list[dict]:
        if not args.library_tests:
            return []
        results = []
        for spec in project["library_tests"]:
            package = spec["package"]
            arguments = [
                "test",
                "--locked",
                "--no-run",
                "--lib",
                "-p",
                package,
                "--message-format=json-render-diagnostics",
            ]
            if spec["features"]:
                arguments.extend(["--features", ",".join(spec["features"])])
            if args.offline:
                arguments.append("--offline")
            log = cargo([*arguments, *config], f"{label}-tests-{package}")
            executable = library_test_executable(log, package)
            execution = run(
                [str(executable), "--test-threads=1", "--color=never"],
                source / "crates" / package,
                timeout=args.build_timeout,
                env=environment,
            )
            results.append(
                {
                    "package": package,
                    "features": spec["features"],
                    "executable_sha256": digest(executable),
                    "execution": execution,
                    "result": library_test_results(execution["stdout"]),
                    "bindings": binding_artifacts(
                        log,
                        args.zstd_source if config else None,
                        output / f"{label}-tests-{package}-dep-info",
                    ),
                }
            )
        return results

    common = [
        "build",
        "--locked",
        "-p",
        project["package"],
        "--message-format=json-render-diagnostics",
    ]
    if args.offline:
        common.append("--offline")
    upstream_log = cargo(common, "upstream")
    binaries = output / "bin"
    binaries.mkdir(exist_ok=True)
    upstream = binaries / f"{name}-upstream"
    shutil.copy2(target / "debug" / project["binary"], upstream)
    shutil.copyfile(source / "Cargo.lock", output / "upstream.lock")
    upstream_bindings = binding_artifacts(
        upstream_log, None, output / "upstream-dep-info"
    )
    upstream_tests = library_tests("upstream", [])
    config = [
        "--config",
        f"patch.crates-io.zstd-sys.path={json.dumps(str(args.zstd_source))}",
    ]
    # `cargo update -p zstd-sys` can also update its transitive dependencies.
    # Preserve every locked version and validate the path substitution strictly.
    patch_zstd_lockfile(source / "Cargo.lock")
    lock = check_locks(output / "upstream.lock", source / "Cargo.lock")
    generated_log = cargo([*common, *config], "generated")
    lock = check_locks(output / "upstream.lock", source / "Cargo.lock")
    generated = binaries / f"{name}-generated"
    shutil.copy2(target / "debug" / project["binary"], generated)
    shutil.copyfile(source / "Cargo.lock", output / "generated.lock")
    generated_bindings = binding_artifacts(
        generated_log, args.zstd_source, output / "generated-dep-info"
    )
    generated_tests = library_tests("generated", config)
    for upstream_test, generated_test in zip(
        upstream_tests, generated_tests, strict=True
    ):
        if upstream_test["result"] != generated_test["result"]:
            raise RuntimeError(
                f"library test results changed for {upstream_test['package']}"
            )
        if sorted(x["features"] for x in upstream_test["bindings"]) != sorted(
            x["features"] for x in generated_test["bindings"]
        ):
            raise RuntimeError("zstd-sys test features changed between builds")
    if sorted(x["features"] for x in upstream_bindings) != sorted(
        x["features"] for x in generated_bindings
    ):
        raise RuntimeError("zstd-sys features changed between builds")
    after = check_archive_source(archive, source)
    after.pop("archive_lock")
    return {
        "project": project,
        "source": after,
        "lock": lock,
        "commands": commands,
        "upstream_bindings": upstream_bindings,
        "generated_bindings": generated_bindings,
        "library_tests": {"upstream": upstream_tests, "generated": generated_tests},
        "upstream_binary": str(upstream),
        "generated_binary": str(generated),
    }


def compare_runtime(
    name: str, upstream: Path, generated: Path, output: Path, python: str
) -> dict:
    if name == "ty":
        results = {
            profile: ty_runtime(binary, output / "ty-cases")
            for profile, binary in [("upstream", upstream), ("generated", generated)]
        }
        for case in TY_CASES:
            for key in ("exit_code", "stdout", "stderr", "source_sha256"):
                if (
                    results["upstream"]["cases"][case][key]
                    != results["generated"]["cases"][case][key]
                ):
                    raise RuntimeError(f"ty runtime differs: {case} {key}")
        return results
    raw, files = wheel()
    compressed, encoder = zstd_compress(raw)
    output.mkdir(parents=True, exist_ok=True)
    (output / "fixture.whl").write_bytes(raw)
    (output / "fixture.whl.zst").write_bytes(compressed)
    results = {
        "encoder": encoder,
        "wheel_sha256": hashlib.sha256(raw).hexdigest(),
        "encoded_wheel_sha256": hashlib.sha256(compressed).hexdigest(),
    }
    with wheel_server(compressed) as (url, requests):
        for profile, binary in [("upstream", upstream), ("generated", generated)]:
            first = len(requests)
            results[profile] = uv_runtime(
                binary, output / f"uv-{profile}", url, files, python
            )
            results[profile]["requests"] = requests[first:]
            if not any(request["method"] == "GET" for request in requests[first:]):
                raise RuntimeError("uv did not download the zstd-encoded wheel")
    if (
        results["upstream"]["installed_fixture_sha256"]
        != results["generated"]["installed_fixture_sha256"]
    ):
        raise RuntimeError("uv installed different fixture bytes")
    corrupted = compressed[:-8]
    (output / "truncated.whl.zst").write_bytes(corrupted)
    results["truncated_frame"] = {"sha256": hashlib.sha256(corrupted).hexdigest()}
    with wheel_server(corrupted) as (url, requests):
        for profile, binary in [("upstream", upstream), ("generated", generated)]:
            first = len(requests)
            rejected = uv_runtime(
                binary,
                output / f"uv-truncated-{profile}",
                url,
                files,
                python,
                expected_exit=1,
            )
            if "unexpected end of file" not in rejected["stderr"]:
                raise RuntimeError(
                    "uv rejected the truncated response for an unexpected reason"
                )
            rejected["requests"] = requests[first:]
            if not any(request["method"] == "GET" for request in requests[first:]):
                raise RuntimeError("uv did not request the truncated zstd fixture")
            results["truncated_frame"][profile] = rejected
    return results


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", choices=["ty", "uv", "all"], default="all")
    parser.add_argument("--cache", type=Path, default=Path(".cache/astral-consumers"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--zstd-source",
        type=Path,
        help="prepared default-profile zstd-sys from verify_zstd_consumer.py",
    )
    parser.add_argument(
        "--zstd-evidence",
        type=Path,
        help="passing evidence.json from the matching default-profile run",
    )
    parser.add_argument(
        "--rust-toolchain",
        default="stable",
        help="installed compatible Rust toolchain (default: stable)",
    )
    parser.add_argument(
        "--python",
        default=sys.executable,
        help="local Python interpreter for the uv install/import case",
    )
    parser.add_argument("--build-timeout", type=int, default=1800)
    parser.add_argument(
        "--library-tests",
        action="store_true",
        help="also compare the pinned compression consumer library test suites",
    )
    parser.add_argument(
        "--offline",
        action="store_true",
        help="require cached archives and Cargo dependencies",
    )
    parser.add_argument(
        "--runtime-only",
        action="store_true",
        help="compare existing binaries; makes no build-validation claim",
    )
    parser.add_argument(
        "--upstream",
        type=Path,
        help="upstream binary with --runtime-only (requires one project)",
    )
    parser.add_argument(
        "--generated", type=Path, help="generated-binding binary with --runtime-only"
    )
    args = parser.parse_args()
    if args.runtime_only:
        if args.library_tests:
            parser.error("--library-tests requires full build validation")
        if args.project == "all" or not args.upstream or not args.generated:
            parser.error(
                "--runtime-only needs one --project, --upstream and --generated"
            )
    elif not args.zstd_source or not args.zstd_evidence:
        parser.error("full validation needs --zstd-source and --zstd-evidence")
    if args.build_timeout <= 0:
        parser.error("--build-timeout must be positive")
    for name in (
        "cache",
        "output",
        "zstd_source",
        "zstd_evidence",
        "upstream",
        "generated",
    ):
        if getattr(args, name) is not None:
            setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    if not args.runtime_only:
        args.cache.mkdir(parents=True, exist_ok=True)
    evidence = {
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "platform": platform.platform(),
        "status": "failed",
        "mode": "runtime-only" if args.runtime_only else "build-and-runtime",
        "projects": {},
    }
    try:
        if args.runtime_only:
            evidence["projects"][args.project] = {
                "runtime": compare_runtime(
                    args.project,
                    args.upstream,
                    args.generated,
                    args.output / args.project,
                    args.python,
                )
            }
        else:
            if "ZSTD_SYS_USE_PKG_CONFIG" in os.environ:
                raise RuntimeError(
                    "unset ZSTD_SYS_USE_PKG_CONFIG to compile the pinned C library"
                )
            evidence["zstd_source"] = check_zstd_source(
                args.zstd_source, args.zstd_evidence
            )
            rust = run(
                ["rustc", "--version", "--verbose"],
                args.output,
                env=os.environ | {"RUSTUP_TOOLCHAIN": args.rust_toolchain},
            )
            evidence["rustc"] = rust
            if (
                f"host: {evidence['zstd_source']['target']}"
                not in rust["stdout"].splitlines()
            ):
                raise RuntimeError(
                    "generated binding target does not match the native Rust host"
                )
            manifest = (
                Path(__file__).resolve().parents[1] / "corpus/consumers/astral.json"
            )
            projects = json.loads(manifest.read_text())
            for name in projects if args.project == "all" else [args.project]:
                built = build_project(name, projects[name], args)
                evidence["projects"][name] = built
                built["runtime"] = compare_runtime(
                    name,
                    Path(built["upstream_binary"]),
                    Path(built["generated_binary"]),
                    args.output / name,
                    args.python,
                )
        evidence["status"] = "passed"
    finally:
        (args.output / "evidence.json").write_text(
            json.dumps(evidence, indent=2) + "\n"
        )
    print(f"{evidence['mode']} passed: {args.output / 'evidence.json'}")


if __name__ == "__main__":
    main()
