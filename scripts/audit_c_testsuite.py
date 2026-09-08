#!/usr/bin/env python3
"""Audit compiler/Toucan acceptance of pinned c-testsuite sources; do not execute them."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime
import hashlib
import json
import math
import os
import platform
import shutil
import signal
import subprocess
import tarfile
import tempfile
import time
import urllib.request
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "corpus/conformance/c-testsuite.json"
MODES = {
    "c90": ["-std=c90"],
    "gnu90": ["-std=gnu90"],
    "c11": ["-std=c11"],
    "gnu11": ["-std=gnu11"],
    "strict_c11": ["-std=c11", "-pedantic-errors"],
}
# These Clang warnings deprecate declarations that remain permitted in C11.
# They do not suppress Toucan errors or C constraint diagnostics.
CLANG_C11_WARNINGS = ["-Wno-strict-prototypes", "-Wno-deprecated-non-prototype"]


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def executable(value: str) -> str:
    resolved = shutil.which(value)
    if resolved is None:
        raise RuntimeError(f"executable not found: {value}")
    return str(Path(resolved).resolve())


def native_target() -> str:
    architecture = {
        "x86_64": "x86_64",
        "AMD64": "x86_64",
        "arm64": "aarch64",
        "aarch64": "aarch64",
    }.get(platform.machine())
    system = {
        "Linux": "unknown-linux-gnu",
        "Darwin": "apple-darwin",
        "Windows": "pc-windows-msvc",
    }.get(platform.system())
    if architecture is None or system is None:
        raise RuntimeError("cannot infer native target; pass --target")
    return f"{architecture}-{system}"


def prepare_archive(manifest: dict, cache: Path, offline: bool) -> Path:
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / f"c-testsuite-{manifest['commit']}.tar.gz"
    if not archive.exists():
        if offline:
            raise RuntimeError(f"archive missing in offline mode: {archive}")
        # Keep incomplete downloads separate from the verified cache entry.
        descriptor, name = tempfile.mkstemp(dir=cache)
        temporary = Path(name)
        try:
            with (
                os.fdopen(descriptor, "wb") as stream,
                urllib.request.urlopen(manifest["archive_url"], timeout=30) as response,
            ):
                size = 0
                while chunk := response.read(1024 * 1024):
                    size += len(chunk)
                    if size > 16 * 1024 * 1024:
                        raise RuntimeError("archive download exceeds 16 MiB")
                    stream.write(chunk)
            if digest(temporary) != manifest["archive_sha256"]:
                raise RuntimeError("downloaded archive SHA-256 does not match the pin")
            temporary.replace(archive)
        finally:
            temporary.unlink(missing_ok=True)
    if digest(archive) != manifest["archive_sha256"]:
        raise RuntimeError(f"cached archive SHA-256 does not match the pin: {archive}")
    return archive


def extract_sources(archive: Path, output: Path, manifest: dict) -> Path:
    """Extract into a fresh report so edits to a previous audit cannot affect this run."""
    destination = output / "source"
    destination.mkdir()
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        if (
            len(members) > 10_000
            or sum(member.size for member in members) > 16 * 1024 * 1024
        ):
            raise RuntimeError("archive exceeds the pinned corpus extraction limits")
        for member in members:
            path = PurePosixPath(member.name)
            if (
                not path.parts
                or path.parts[0] != manifest["archive_root"]
                or path.is_absolute()
                or ".." in path.parts
                or "\\" in member.name
                or not (member.isdir() or member.isfile())
            ):
                raise RuntimeError(f"unexpected archive member: {member.name}")
        bundle.extractall(destination, filter="data")
    return destination / manifest["archive_root"]


def run(command: list[str], directory: Path, stem: str, timeout: float) -> dict:
    """Retain diagnostics on rejection, crashes, startup failure, and timeout."""
    stdout = directory / f"{stem}.stdout"
    stderr = directory / f"{stem}.stderr"
    record = {
        "command": command,
        "cwd": str(directory),
        "stdout": str(stdout),
        "stderr": str(stderr),
        "exit_code": None,
        "timeout": False,
    }
    started = time.monotonic()
    try:
        with stdout.open("wb") as out, stderr.open("wb") as err:
            process = subprocess.Popen(
                command,
                cwd=directory,
                stdin=subprocess.DEVNULL,
                stdout=out,
                stderr=err,
                start_new_session=os.name == "posix",
            )
            try:
                record["exit_code"] = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                record["timeout"] = True
                if os.name == "posix":
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                else:
                    # The compiler driver can have a running child compiler.
                    try:
                        subprocess.run(
                            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                            stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL,
                            timeout=5,
                            check=False,
                        )
                    except (OSError, subprocess.TimeoutExpired) as error:
                        record["process_tree_kill_error"] = str(error)
                    finally:
                        if process.poll() is None:
                            process.kill()
                record["exit_code"] = process.wait()
    except OSError as error:
        record["start_error"] = str(error)
    finally:
        record["seconds"] = time.monotonic() - started
    diagnostics = stderr.read_text(errors="replace").lower()
    record["crash_diagnostic"] = any(
        message in diagnostics
        for message in [
            "internal compiler error:",
            "please submit a bug report",
            "thread 'main' panicked at",
        ]
    )
    return record


def accepted(result: dict) -> bool:
    return (
        result["exit_code"] == 0
        and not result["timeout"]
        and "start_error" not in result
    )


def tool_failure(result: dict) -> bool:
    code = result["exit_code"]
    # Rust panic exits with 101; signal exits are negative on POSIX. Ordinary
    # compiler and CLI diagnostics return 1, while argument errors often return 2.
    return (
        result["timeout"]
        or result["crash_diagnostic"]
        or "start_error" in result
        or (code is not None and (code < 0 or code == 101 or code >= 128))
    )


def compiler_flags(args: argparse.Namespace, compiler: str, mode: str) -> list[str]:
    flags = [*args.cc_arg, *getattr(args, f"{compiler}_arg"), *MODES[mode]]
    if compiler == "clang" and mode == "strict_c11":
        flags += CLANG_C11_WARNINGS
    return flags


def toucan_command(args: argparse.Namespace, source: Path) -> list[str]:
    """Check the selected compiler's source with its corresponding frontend profile."""
    return [
        args.toucan,
        "check",
        "--target",
        args.target,
        "--compiler",
        args.compiler,
        str(source),
    ]


def audit(source: Path, args: argparse.Namespace, manifest: dict) -> dict:
    directory = args.output / "cases" / source.stem
    directory.mkdir(parents=True)
    sidecars = {}
    for suffix in ["tags", "otags"]:
        path = source.with_suffix(f".c.{suffix}")
        if path.exists():
            sidecars[suffix] = {
                "path": str(path),
                "sha256": digest(path),
                "text": path.read_text(encoding="utf-8"),
            }
    origin = dict(
        line.split("=", 1)
        for line in sidecars.get("otags", {}).get("text", "").splitlines()
        if "=" in line
    )
    result = {
        "name": source.name,
        "source": str(source),
        "source_sha256": digest(source),
        "source_url": f"{manifest['repository']}/blob/{manifest['commit']}/{manifest['tests_directory']}/{source.name}",
        "sidecars": sidecars,
        "origin": origin or None,
        "compilers": {},
    }
    for compiler in ["gcc", "clang"]:
        result["compilers"][compiler] = {
            mode: run(
                [
                    getattr(args, compiler),
                    *compiler_flags(args, compiler, mode),
                    "-fsyntax-only",
                    str(source),
                ],
                directory,
                f"{compiler}-{mode}",
                args.timeout,
            )
            for mode in MODES
        }
    cc = args.preprocessor
    flags = compiler_flags(args, cc, args.dialect)
    preprocessed = directory / "source.i"
    result["preprocess"] = run(
        [getattr(args, cc), *flags, "-E", "-P", str(source), "-o", str(preprocessed)],
        directory,
        "preprocess",
        args.timeout,
    )
    if accepted(result["preprocess"]):
        result["preprocessed_sha256"] = digest(preprocessed)
        result["preprocessed_bytes"] = preprocessed.stat().st_size
        result["preprocessed_oracle"] = run(
            [getattr(args, cc), *flags, "-fsyntax-only", str(preprocessed)],
            directory,
            "preprocessed-oracle",
            args.timeout,
        )
        result["toucan"] = run(
            toucan_command(args, preprocessed),
            directory,
            "toucan",
            args.timeout,
        )
    result["both_accept"] = {
        mode: all(accepted(result["compilers"][cc][mode]) for cc in ["gcc", "clang"])
        for mode in MODES
    }
    result["eligible"] = result["both_accept"][args.dialect] and accepted(
        result.get("preprocessed_oracle", {"exit_code": None})
    )
    result["difference"] = result["eligible"] and not accepted(
        result.get("toucan", {"exit_code": None})
    )
    commands = [
        command
        for compiler in result["compilers"].values()
        for command in compiler.values()
    ]
    commands += [
        result[name]
        for name in ["preprocess", "preprocessed_oracle", "toucan"]
        if name in result
    ]
    result["tool_failure"] = any(tool_failure(command) for command in commands)
    # If a compiler accepted the original source, failure to preprocess or parse
    # that same compiler's output is an audit problem, not a Toucan mismatch.
    result["oracle_pipeline_failure"] = accepted(
        result["compilers"][cc][args.dialect]
    ) and not accepted(result.get("preprocessed_oracle", {"exit_code": None}))
    write_json(directory / "result.json", result)
    return result


def summarize(cases: list[dict], changed_tools: list[str]) -> dict:
    """Classify both exploratory and pedantic-positive cases without an exception list."""
    strict = [
        case for case in cases if case["eligible"] and case["both_accept"]["strict_c11"]
    ]
    return {
        "source_count": len(cases),
        "both_accept": {
            mode: sum(case["both_accept"][mode] for case in cases) for mode in MODES
        },
        "eligible_count": sum(case["eligible"] for case in cases),
        "toucan_checked": sum("toucan" in case for case in cases),
        "toucan_accepted": sum(
            accepted(case["toucan"]) for case in cases if "toucan" in case
        ),
        "differences": [case["name"] for case in cases if case["difference"]],
        "strict_eligible_count": len(strict),
        "strict_toucan_accepted": sum(accepted(case["toucan"]) for case in strict),
        "strict_differences": [case["name"] for case in strict if case["difference"]],
        "tool_failures": [case["name"] for case in cases if case["tool_failure"]],
        "oracle_pipeline_failures": [
            case["name"] for case in cases if case["oracle_pipeline_failure"]
        ],
        "changed_tools": changed_tools,
    }


def audit_failed(
    summary: dict,
    *,
    fail_on_difference: bool = False,
    fail_on_strict_difference: bool = False,
) -> bool:
    """Keep infrastructure failures fatal under either acceptance policy."""
    return bool(
        summary["tool_failures"]
        or summary["oracle_pipeline_failures"]
        or summary["changed_tools"]
        or (fail_on_difference and summary["differences"])
        or (fail_on_strict_difference and summary["strict_differences"])
    )


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--toucan",
        default=str(ROOT / "target/release/toucan"),
        help="Toucan executable (default: target/release/toucan)",
    )
    parser.add_argument(
        "--gcc",
        default=os.environ.get("TOUCAN_GCC", "gcc"),
        help="GCC executable; use versioned GCC on macOS",
    )
    parser.add_argument("--clang", default="clang", help="Clang executable")
    parser.add_argument(
        "--cc-arg",
        action="append",
        default=[],
        help="argument for both C compilers (repeat; use --cc-arg=VALUE for flags)",
    )
    parser.add_argument(
        "--gcc-arg",
        action="append",
        default=[],
        help="additional GCC argument (repeat)",
    )
    parser.add_argument(
        "--clang-arg",
        action="append",
        default=[],
        help="additional Clang argument (repeat)",
    )
    parser.add_argument(
        "--target", help="Toucan target triple; defaults to the native host"
    )
    parser.add_argument(
        "--preprocessor",
        choices=["gcc", "clang"],
        default="gcc",
        help="compiler providing the exact input checked by Toucan",
    )
    parser.add_argument(
        "--dialect",
        choices=["c90", "gnu90", "c11", "gnu11"],
        default="c11",
        help="dialect for preprocessing and the acceptance comparison",
    )
    parser.add_argument(
        "--cache",
        type=Path,
        default=ROOT / "corpus/cache/conformance",
        help="verified download cache",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="new or empty report directory; defaults to a timestamped directory under --cache",
    )
    parser.add_argument(
        "--offline",
        action="store_true",
        help="require the pinned archive to be present in --cache",
    )
    parser.add_argument(
        "--case",
        action="append",
        default=[],
        help="case name such as 00162 or 00162.c (repeat)",
    )
    parser.add_argument(
        "--limit", type=int, help="run the first N selected cases for a smoke test"
    )
    parser.add_argument(
        "--workers", type=int, default=4, help="parallel source cases (default: 4)"
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=10,
        help="seconds allowed per compiler/Toucan command (default: 10)",
    )
    parser.add_argument(
        "--fail-on-difference",
        action="store_true",
        help="return nonzero if Toucan rejects an input accepted by both selected-dialect oracles",
    )
    parser.add_argument(
        "--fail-on-strict-difference",
        action="store_true",
        help="return nonzero for eligible Toucan rejections accepted by both pedantic C11 oracles",
    )
    args = parser.parse_args()
    if (
        args.workers < 1
        or not math.isfinite(args.timeout)
        or args.timeout <= 0
        or (args.limit is not None and args.limit < 1)
    ):
        parser.error("workers, timeout, and limit must be positive")
    return args


def main() -> int:
    args = arguments()
    started = time.monotonic()
    timestamp = datetime.datetime.now(datetime.UTC)
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    args.target = args.target or native_target()
    args.compiler = args.preprocessor
    for name in ["toucan", "gcc", "clang"]:
        setattr(args, name, executable(getattr(args, name)))
    args.cache = args.cache.resolve()
    args.output = (
        args.output or args.cache / "audits" / timestamp.strftime("%Y%m%dT%H%M%S.%fZ")
    ).resolve()
    if args.output.exists() and any(args.output.iterdir()):
        raise RuntimeError(f"report directory is not empty: {args.output}")
    archive = prepare_archive(manifest, args.cache, args.offline)
    args.output.mkdir(parents=True, exist_ok=True)
    sources = extract_sources(archive, args.output, manifest)
    available = sorted((sources / manifest["tests_directory"]).glob("*.c"))
    if len(available) != manifest["source_count"]:
        raise RuntimeError("source inventory does not match the pinned manifest")
    names = {name if name.endswith(".c") else name + ".c" for name in args.case}
    missing = names - {source.name for source in available}
    if missing:
        raise RuntimeError(f"unknown source cases: {sorted(missing)}")
    selected = [source for source in available if not names or source.name in names]
    if args.limit:
        selected = selected[: args.limit]
    tools = {}
    for name in ["toucan", "gcc", "clang"]:
        path = Path(getattr(args, name))
        version = run(
            [str(path), "--version"], args.output, f"{name}-version", args.timeout
        )
        if not accepted(version):
            raise RuntimeError(f"cannot query {name} version; see {version['stderr']}")
        tools[name] = {
            "path": str(path),
            "sha256": digest(path),
            "version": Path(version["stdout"]).read_text(errors="replace").strip(),
            "version_command": version,
        }
    if (
        "clang" in tools["gcc"]["version"].lower()
        or "clang" not in tools["clang"]["version"].lower()
    ):
        raise RuntimeError(
            "--gcc must name GCC and --clang must name Clang; macOS /usr/bin/gcc is Clang"
        )
    probe = args.output / "compiler-probe.c"
    probe.write_text("int main(void) { return 0; }\n", encoding="utf-8")
    for compiler in ["gcc", "clang"]:
        validations = {}
        for mode in MODES:
            validations[mode] = run(
                [
                    getattr(args, compiler),
                    *compiler_flags(args, compiler, mode),
                    "-fsyntax-only",
                    str(probe),
                ],
                args.output,
                f"{compiler}-validate-{mode}",
                args.timeout,
            )
            if not accepted(validations[mode]):
                raise RuntimeError(
                    f"{compiler} cannot compile the {mode} control; check compiler arguments and {validations[mode]['stderr']}"
                )
        tools[compiler]["configuration_controls"] = validations
    toucan_control = run(
        toucan_command(args, probe),
        args.output,
        "toucan-validate",
        args.timeout,
    )
    if not accepted(toucan_control):
        raise RuntimeError(
            f"Toucan cannot check the valid C control; see {toucan_control['stderr']}"
        )
    tools["toucan"]["configuration_control"] = toucan_control
    report = {
        "schema_version": 1,
        "started_utc": timestamp.isoformat(),
        "manifest": manifest,
        "manifest_sha256": digest(MANIFEST),
        "runner_sha256": digest(Path(__file__)),
        "archive": str(archive),
        "tools": tools,
        "host": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "configuration": {
            name: getattr(args, name)
            for name in [
                "target",
                "compiler",
                "preprocessor",
                "dialect",
                "cc_arg",
                "gcc_arg",
                "clang_arg",
                "case",
                "limit",
                "workers",
                "timeout",
                "fail_on_difference",
                "fail_on_strict_difference",
            ]
        },
        "environment": {
            name: os.environ[name]
            for name in [
                "CPATH",
                "C_INCLUDE_PATH",
                "CPLUS_INCLUDE_PATH",
                "SDKROOT",
                "MACOSX_DEPLOYMENT_TARGET",
                "GCC_EXEC_PREFIX",
                "COMPILER_PATH",
            ]
            if name in os.environ
        },
        "method": "Original sources are classified with GCC and Clang in C11, GNU11, and pedantic C11 modes. The chosen compiler's -E -P output is validated by that same compiler before the Toucan acceptance comparison, which selects the matching GNU or Clang compiler profile. Sources are never linked or executed; this is not an ABI, runtime, or Toucan preprocessing conformance test.",
        "clang_strict_warning_exceptions": CLANG_C11_WARNINGS,
        "cases": [],
    }
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        for result in executor.map(
            lambda source: audit(source, args, manifest), selected
        ):
            report["cases"].append(result)
            if len(report["cases"]) % 25 == 0:
                print(f"Checked {len(report['cases'])}/{len(selected)}", flush=True)
    changed_tools = [
        name
        for name in tools
        if digest(Path(tools[name]["path"])) != tools[name]["sha256"]
    ]
    report["summary"] = summarize(report["cases"], changed_tools)
    report["seconds"] = time.monotonic() - started
    write_json(args.output / "evidence.json", report)
    print(json.dumps(report["summary"], indent=2))
    print(f"Evidence: {args.output / 'evidence.json'}")
    return int(
        audit_failed(
            report["summary"],
            fail_on_difference=args.fail_on_difference,
            fail_on_strict_difference=args.fail_on_strict_difference,
        )
    )


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError, tarfile.TarError) as error:
        raise SystemExit(f"audit failed: {error}") from error
