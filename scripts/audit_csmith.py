#!/usr/bin/env python3
"""Audit pinned or supplied Csmith sources; compiler rejection is an exclusion."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime
import hashlib
import json
import os
import platform
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

from compiler_diagnostics import has_crash_diagnostic

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "corpus/conformance/csmith/manifest.json"
STRICT = [
    "-std=c11",
    "-pedantic-errors",
    "-Werror=pointer-sign",
    "-Werror=incompatible-pointer-types",
]


def digest(path: Path | str) -> str:
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def classify(
    code: int | None,
    diagnostic: str,
    *,
    timeout=False,
    start_error=False,
    kind="syntax",
) -> str:
    if start_error:
        return "tool_error"
    if timeout:
        return "timeout"
    if code == -signal.SIGXFSZ:
        return "output_limit"
    if code is None or has_crash_diagnostic(diagnostic):
        return "crash"
    if code < 0:
        return "crash"
    if code == 0:
        return "accepted"
    if kind == "runtime":
        return (
            "sanitizer_failure"
            if "runtime error:" in diagnostic.lower()
            else "runtime_failure"
        )
    return "rejected" if code == 1 and kind == "syntax" else "tool_error"


def run(
    command: list[str], directory: Path, stem: str, *, timeout=30, kind="syntax"
) -> dict:
    """Bound child output/time and retain failures without treating crashes as rejection."""
    command = ["prlimit", "--fsize=268435456:268435456", "--core=0:0", "--", *command]
    out, err = directory / f"{stem}.stdout", directory / f"{stem}.stderr"
    record = {
        "command": command,
        "cwd": str(directory),
        "stdout": str(out),
        "stderr": str(err),
        "kind": kind,
    }
    started = time.monotonic()
    try:
        with out.open("wb") as stdout, err.open("wb") as stderr:
            process = subprocess.Popen(
                command,
                cwd=directory,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                start_new_session=True,
                env=os.environ
                | {
                    "LC_ALL": "C",
                    "SOURCE_DATE_EPOCH": "0",
                    "UBSAN_OPTIONS": "halt_on_error=1:print_stacktrace=1",
                },
            )
            try:
                record["exit_code"] = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                record["exit_code"] = process.wait()
                record["timeout"] = True
    except OSError as error:
        record["start_error"] = str(error)
    record["seconds"] = time.monotonic() - started
    diagnostic = err.read_text(errors="replace")
    record["status"] = classify(
        record.get("exit_code"),
        out.read_text(errors="replace") + "\n" + diagnostic,
        timeout=record.get("timeout", False),
        start_error="start_error" in record,
        kind=kind,
    )
    record["stdout_sha256"], record["stderr_sha256"] = digest(out), digest(err)
    if diagnostic:
        record["diagnostic"] = diagnostic[:65_536]
        record["diagnostic_truncated"] = len(diagnostic) > 65_536
    return record


def parity(normal: dict, retained: dict) -> bool:
    if normal["status"] != retained["status"]:
        return False
    if normal["status"] == "accepted":
        return bool(normal.get("unit_sha256")) and normal[
            "unit_sha256"
        ] == retained.get("unit_sha256")
    diagnostic = normal.get("result", {}).get("diagnostic")
    return (
        normal["status"] == "rejected"
        and isinstance(diagnostic, str)
        and bool(diagnostic)
        and diagnostic == retained.get("result", {}).get("diagnostic")
    )


def probe(args, source: Path, compiler: str, directory: Path, mode: str) -> dict:
    output = directory / f"{compiler}-{mode}.unit"
    result = run(
        [str(args.runner), str(source), compiler, args.target, mode, str(output)],
        directory,
        f"{compiler}-{mode}",
        timeout=args.timeout,
    )
    try:
        result["result"] = json.loads(Path(result["stdout"]).read_text())
    except (ValueError, OSError) as error:
        result["result"] = {"status": "tool_error", "diagnostic": str(error)}
    if not isinstance(result["result"], dict):
        result["result"] = {
            "status": "tool_error",
            "diagnostic": "probe result is not an object",
        }
    reported = result["result"].get("status")
    if result["status"] in ("accepted", "rejected") and result["status"] != reported:
        result["status"] = "tool_error"
    if result["status"] == "accepted":
        if not output.is_file() or result["result"].get("retained") is not (
            mode == "retained"
        ):
            result["status"] = "tool_error"
        else:
            result["unit_sha256"], result["unit_bytes"] = (
                digest(output),
                output.stat().st_size,
            )
    return result


def pinned_files(manifest: dict, directory: Path) -> tuple[list[dict], list[Path]]:
    cases = manifest["cases"]
    headers = [directory / item["path"] for item in manifest["headers"]]
    for item in [*cases, *manifest["headers"]]:
        path = directory / item["path"]
        if not path.is_file() or digest(path) != item["sha256"]:
            raise RuntimeError(f"fixture differs from its pinned hash: {path}")
    return cases, headers


def check_case(args, source: Path, directory: Path, seed, tools: dict) -> dict:
    row = {
        "seed": seed,
        "source": str(source),
        "source_sha256": digest(source),
        "source_bytes": source.stat().st_size,
        "compilers": {},
    }
    for name in ("gcc", "clang"):
        command = [
            tools[name]["path"],
            *STRICT,
            "-I",
            str(args.headers),
            "-fsyntax-only",
            str(source),
        ]
        row["compilers"][name] = {
            "syntax": run(command, directory, f"{name}-strict", timeout=args.timeout)
        }
    row["eligible"] = all(
        value["syntax"]["status"] == "accepted" for value in row["compilers"].values()
    )
    if not row["eligible"]:
        row["exclusion"] = (
            "compiler_rejection"
            if any(
                value["syntax"]["status"] == "rejected"
                for value in row["compilers"].values()
            )
            else "compiler_failure"
        )
        return row
    for name, native in row["compilers"].items():
        compiler = tools[name]["path"]
        preprocessed = directory / f"{name}.i"
        native["preprocess"] = run(
            [
                compiler,
                *STRICT,
                "-I",
                str(args.headers),
                "-E",
                str(source),
                "-o",
                str(preprocessed),
            ],
            directory,
            f"{name}-preprocess",
            timeout=args.timeout,
            kind="preprocess",
        )
        if native["preprocess"]["status"] != "accepted":
            continue
        native["preprocessed_sha256"] = digest(preprocessed)
        # Clang rejects its own generated GNU line markers under -pedantic-errors.
        # This exception applies only to generated metadata, never source eligibility.
        marker_flags = ["-Wno-gnu-line-marker"] if name == "clang" else []
        native["preprocessed_syntax"] = run(
            [compiler, *STRICT, *marker_flags, "-fsyntax-only", str(preprocessed)],
            directory,
            f"{name}-preprocessed-strict",
            timeout=args.timeout,
        )
        if native["preprocessed_syntax"]["status"] != "accepted":
            continue
        native["dependencies"] = {}
        for dependency in set(
            re.findall(
                r'^#\s+\d+\s+"([^"\n]+)"', preprocessed.read_text(), re.MULTILINE
            )
        ):
            path = Path(dependency)
            if path.is_file():
                native["dependencies"][dependency] = digest(path)
        native["cli"] = run(
            [
                str(args.toucan),
                "check",
                "--compiler",
                name,
                "--target",
                args.target,
                str(preprocessed),
            ],
            directory,
            f"{name}-cli",
            timeout=args.timeout,
        )
        native["frontend"] = {
            mode: probe(args, preprocessed, name, directory, mode)
            for mode in ("normal", "retained")
        }
        native["parity"] = (
            parity(native["frontend"]["normal"], native["frontend"]["retained"])
            and native["cli"]["status"] == native["frontend"]["normal"]["status"]
        )
    return row


def execute_case(args, row: dict, tools: dict) -> None:
    source = Path(row["source"])
    directory = source.parent
    checksums = []
    row["native_execution"] = {}
    for compiler in ("gcc", "clang"):
        for label, flags in (
            ("O0", ["-O0"]),
            ("O2", ["-O2"]),
            (
                "ubsan",
                ["-O1", "-g", "-fsanitize=undefined", "-fno-sanitize-recover=all"],
            ),
        ):
            key = f"{compiler}-{label}"
            binary = directory / key
            result = {
                "compile": run(
                    [
                        tools[compiler]["path"],
                        *STRICT,
                        "-I",
                        str(args.headers),
                        *flags,
                        str(source),
                        "-o",
                        str(binary),
                    ],
                    directory,
                    f"{key}-compile",
                    timeout=args.compile_timeout,
                    kind="compile",
                )
            }
            if result["compile"]["status"] == "accepted":
                result["binary_sha256"] = digest(binary)
                result["run"] = run(
                    [str(binary)],
                    directory,
                    f"{key}-run",
                    timeout=args.runtime_timeout,
                    kind="runtime",
                )
                checksum = re.fullmatch(
                    r"checksum = ([0-9a-fA-F]+)\n",
                    Path(result["run"]["stdout"]).read_text(),
                )
                result["checksum"] = checksum[1].lower() if checksum else None
                if result["run"]["status"] == "accepted" and checksum:
                    checksums.append(result["checksum"])
            row["native_execution"][key] = result
    row["native_crc_agreement"] = len(checksums) == 6 and len(set(checksums)) == 1


def summarize(report: dict) -> dict:
    result = {
        "eligible": 0,
        "excluded": [],
        "accepted_profiles": 0,
        "frontend_failures": [],
        "pipeline_failures": [],
        "parity_failures": [],
        "tool_failures": [],
        "runtime_failures": [],
        "native_executions": 0,
    }

    def visit(value, path):
        if isinstance(value, dict):
            if value.get("status") in (
                "crash",
                "timeout",
                "output_limit",
                "tool_error",
            ):
                result["tool_failures"].append(
                    {"path": path, "status": value["status"]}
                )
            for key, child in value.items():
                visit(child, f"{path}/{key}")
        elif isinstance(value, list):
            for index, child in enumerate(value):
                visit(child, f"{path}/{index}")

    visit(report["cases"], "cases")
    for case in report["cases"]:
        if not case.get("eligible"):
            result["excluded"].append(case["seed"])
            continue
        result["eligible"] += 1
        for compiler, native in case["compilers"].items():
            label = {"seed": case["seed"], "compiler": compiler}
            if "frontend" not in native:
                result["pipeline_failures"].append(label)
                continue
            normal = native["frontend"]["normal"]
            if normal["status"] == "accepted":
                result["accepted_profiles"] += 1
            else:
                result["frontend_failures"].append(
                    label
                    | {
                        "status": normal["status"],
                        "diagnostic": normal.get("result", {}).get("diagnostic"),
                    }
                )
            if not native["parity"]:
                result["parity_failures"].append(label)
        if "native_execution" in case:
            result["native_executions"] += sum(
                value.get("run", {}).get("status") == "accepted"
                for value in case["native_execution"].values()
            )
            if not case["native_crc_agreement"]:
                result["runtime_failures"].append(case["seed"])
    return result


def executable(path: str | Path) -> str:
    resolved = shutil.which(str(path))
    if not resolved:
        raise RuntimeError(f"executable not found: {path}")
    return str(Path(resolved).resolve())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", type=Path, required=True, help="new evidence directory"
    )
    parser.add_argument("--toucan", type=Path, default=ROOT / "target/release/toucan")
    parser.add_argument(
        "--runner", type=Path, default=ROOT / "target/release/examples/audit_csmith"
    )
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    parser.add_argument(
        "--sources", type=Path, help="directory of untouched generated .c inputs"
    )
    parser.add_argument(
        "--csmith",
        type=Path,
        help="optional installed generator for a fresh bounded corpus",
    )
    parser.add_argument(
        "--headers",
        type=Path,
        help="Csmith runtime headers (defaults to pinned copies)",
    )
    parser.add_argument(
        "--count", type=int, default=32, help="number of programs in generator mode"
    )
    parser.add_argument("--first-seed", type=int, default=2026090801)
    parser.add_argument("--runtime-count", type=int, default=1)
    parser.add_argument("--gcc", default="gcc")
    parser.add_argument("--clang", default="clang")
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--compile-timeout", type=float, default=60)
    parser.add_argument("--runtime-timeout", type=float, default=5)
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "aarch64"):
        parser.error(
            "this native audit currently supports 64-bit Linux; no platform is silently skipped"
        )
    if args.csmith and args.sources:
        parser.error("choose --sources or --csmith")
    if (
        not 1 <= args.count <= 10_000
        or not 0 <= args.runtime_count <= 100
        or not 1 <= args.workers <= 32
    ):
        parser.error("count/workers/runtime-count exceed the bounded audit range")
    if not 0 <= args.first_seed <= 2**32 - args.count or any(
        value <= 0 or value > 3600
        for value in (args.timeout, args.compile_timeout, args.runtime_timeout)
    ):
        parser.error("seeds and timeout values must be within the bounded audit range")
    args.target = f"{platform.machine()}-unknown-linux-gnu"
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    args.toucan, args.runner = (
        Path(executable(args.toucan)),
        Path(executable(args.runner)),
    )
    executable("prlimit")
    manifest = json.loads(args.manifest.read_text())
    pinned, header_files = pinned_files(manifest, args.manifest.parent)
    args.headers = (args.headers or args.manifest.parent / "runtime").resolve()
    if not args.headers.is_dir():
        raise RuntimeError("runtime header directory is missing")
    report = {
        "schema_version": 1,
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "target": args.target,
        "manifest_sha256": digest(args.manifest),
        "generator": manifest["generator"],
        "tools": {},
        "header_hashes": {str(path): digest(path) for path in header_files},
        "cases": [],
        "method": {
            "eligibility": STRICT,
            "clang_preprocessed_exception": "-Wno-gnu-line-marker only for compiler-generated markers; original source eligibility is unchanged",
            "runtime": "first eligible inputs, GCC/Clang O0/O2 and O1+UBSan; equal CRCs are native oracle evidence, not execution by Toucan",
            "retention": "public parse_file; compare complete declaration SHA-256, never truncated prefixes",
            "limits": {
                "command_seconds": args.timeout,
                "compile_seconds": args.compile_timeout,
                "runtime_seconds": args.runtime_timeout,
                "child_output_bytes": 256 * 1024 * 1024,
                "source_bytes": 4 * 1024 * 1024,
            },
        },
    }
    for name, path in {
        "gcc": args.gcc,
        "clang": args.clang,
        "toucan": args.toucan,
        "runner": args.runner,
        **({"csmith": args.csmith} if args.csmith else {}),
    }.items():
        path = executable(path)
        tool = {"path": path, "sha256": digest(path)}
        if name != "runner":
            version = run(
                [path, "--version"], args.output, f"{name}-version", kind="version"
            )
            if version["status"] != "accepted":
                raise RuntimeError(f"cannot identify {name}: {version}")
            tool["version"] = Path(version["stdout"]).read_text()
        report["tools"][name] = tool
    if (
        "Free Software Foundation" not in report["tools"]["gcc"]["version"]
        or "clang" not in report["tools"]["clang"]["version"].lower()
    ):
        raise RuntimeError(
            "the gcc and clang oracles must identify those compiler families"
        )
    required = not args.sources and not args.csmith
    if args.sources:
        sources = sorted(args.sources.resolve().glob("*.c"))
        if not sources or len(sources) > 10_000:
            raise RuntimeError("source directory must contain 1–10000 .c files")
        jobs = [(path.stem, path, None) for path in sources]
    elif args.csmith:
        jobs = [
            (seed, None, None)
            for seed in range(args.first_seed, args.first_seed + args.count)
        ]
    else:
        jobs = [
            (case["seed"], args.manifest.parent / case["path"], case["sha256"])
            for case in pinned
        ]

    def case(indexed_job):
        index, (seed, original, expected) = indexed_job
        directory = args.output / f"{index:05d}"
        directory.mkdir()
        source = directory / "source.c"
        generation = None
        if original:
            if original.stat().st_size > 4 * 1024 * 1024:
                raise RuntimeError(f"source exceeds 4 MiB: {original}")
            shutil.copyfile(original, source)
            if expected and digest(source) != expected:
                raise RuntimeError(f"copied fixture hash mismatch: {original}")
        else:
            generation = run(
                [
                    report["tools"]["csmith"]["path"],
                    "--seed",
                    str(seed),
                    *manifest["generator"]["options"],
                    "--output",
                    str(source),
                ],
                directory,
                "generate",
                timeout=args.timeout,
                kind="generation",
            )
            if (
                generation["status"] != "accepted"
                or not source.is_file()
                or source.stat().st_size > 4 * 1024 * 1024
            ):
                return {
                    "seed": seed,
                    "eligible": False,
                    "generator": generation,
                    "generation_failure": True,
                }
        row = check_case(args, source, directory, seed, report["tools"])
        row["generator"] = generation or {
            "source": str(original),
            "source_sha256": digest(original),
        }
        info = directory / "platform.info"
        if info.is_file():
            row["platform_info"] = info.read_text()
        return row

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        for row in pool.map(case, enumerate(jobs)):
            report["cases"].append(row)
            write_json(args.output / "evidence.json", report)
    for row in [row for row in report["cases"] if row.get("eligible")][
        : args.runtime_count
    ]:
        execute_case(args, row, report["tools"])
        write_json(args.output / "evidence.json", report)
    report["summary"] = summarize(report)
    report["summary"]["changed_tools"] = [
        name
        for name, tool in report["tools"].items()
        if digest(tool["path"]) != tool["sha256"]
    ]
    changed_inputs = []
    for row in report["cases"]:
        if "source" in row and digest(row["source"]) != row["source_sha256"]:
            changed_inputs.append(row["source"])
        for native in row.get("compilers", {}).values():
            changed_inputs.extend(
                path
                for path, expected in native.get("dependencies", {}).items()
                if not Path(path).is_file() or digest(path) != expected
            )
    report["summary"]["changed_inputs"] = sorted(set(changed_inputs))
    report["summary"]["required_fixture_exclusions"] = (
        report["summary"]["excluded"] if required else []
    )
    report["summary"]["generation_failures"] = [
        row["seed"] for row in report["cases"] if row.get("generation_failure")
    ]
    write_json(args.output / "evidence.json", report)
    print(json.dumps(report["summary"], indent=2))
    print(f"Evidence: {args.output / 'evidence.json'}")
    failures = (
        "frontend_failures",
        "pipeline_failures",
        "parity_failures",
        "tool_failures",
        "runtime_failures",
        "changed_tools",
        "changed_inputs",
        "required_fixture_exclusions",
        "generation_failures",
    )
    return int(
        not report["summary"]["eligible"]
        or any(report["summary"][key] for key in failures)
    )


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError) as error:
        print(f"audit failed: {error}", file=sys.stderr)
        raise SystemExit(2) from error
