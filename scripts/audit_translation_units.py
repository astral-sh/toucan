#!/usr/bin/env python3
"""Audit seven pinned C translation units through both public preprocessing routes."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import platform
import shlex
import shutil
import signal
import subprocess
import sys
import tarfile
import time
from pathlib import Path

# The devbox may set PYTHONSAFEPATH; resolve sibling helpers from this script,
# independently of the working directory or an ambient PYTHONPATH.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from prepare_corpus import digest
from verify_corpus import native_target

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "corpus/translation-units.json"
MAX_REPORT_BYTES = 1024 * 1024


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def run(
    command: list[str], cwd: Path, directory: Path, stem: str, timeout: int
) -> dict:
    """Capture failures and kill the entire compiler process group on timeout."""
    stdout, stderr = directory / f"{stem}.stdout", directory / f"{stem}.stderr"
    result = {
        "command": command,
        "cwd": str(cwd),
        "stdout": str(stdout),
        "stderr": str(stderr),
        "exit_code": None,
        "timeout": False,
    }
    env = {**os.environ, "LC_ALL": "C", "SOURCE_DATE_EPOCH": "0"}
    timer = shutil.which("time")
    rss = directory / f"{stem}.rss"
    measured_command = (
        [timer, "-f", "%M", "-o", str(rss), *command] if timer else command
    )
    result["measurement_command"] = measured_command
    started = time.monotonic()
    with stdout.open("wb") as out, stderr.open("wb") as err:
        try:
            process = subprocess.Popen(
                measured_command,
                cwd=cwd,
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=out,
                stderr=err,
                start_new_session=True,
            )
            try:
                result["exit_code"] = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                result["timeout"] = True
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                result["exit_code"] = process.wait()
        except OSError as error:
            result["start_error"] = str(error)
    result["execution_seconds"] = time.monotonic() - started
    result["peak_rss_kib"] = None
    if rss.exists() and not result["timeout"]:
        try:
            result["peak_rss_kib"] = int(rss.read_text().splitlines()[-1])
        except (ValueError, IndexError):
            result["measurement_error"] = "missing GNU time maximum RSS"
    elif not timer:
        result["measurement_error"] = "external time executable unavailable"
    with stderr.open("rb") as stream:
        stream.seek(max(0, stderr.stat().st_size - 8192))
        result["diagnostic_tail"] = stream.read().decode(errors="replace")
    return result


def checked_run(
    command: list[str], cwd: Path, directory: Path, stem: str, timeout: int
) -> dict:
    result = run(command, cwd, directory, stem, timeout)
    if result["exit_code"] != 0 or result["timeout"]:
        raise RuntimeError(f"{stem} failed: {result}")
    return result


def resolve(value: str, cwd: Path) -> Path:
    return (cwd / value).resolve()


def compilation(
    project: dict, case: dict, source: Path
) -> tuple[list[str], Path, Path]:
    """Read the build's command without executing shell syntax from its metadata."""
    build = Path(project["build"])
    candidates = []
    if case["command_kind"] == "cmake":
        provenance = build / "compile_commands.json"
        for entry in json.loads(provenance.read_text()):
            cwd = Path(entry["directory"])
            if resolve(entry["file"], cwd) != source:
                continue
            argv = entry.get("arguments") or shlex.split(entry["command"])
            marker = f"CMakeFiles/{case['cmake_target']}.dir/"
            if not any(marker in part for part in [*argv, entry.get("output", "")]):
                continue
            candidates.append((argv, cwd, provenance))
    elif case["command_kind"] == "make":
        commands = [
            c
            for c in project["commands"]
            if "-n" in c["command"] and "V=1" in c["command"]
        ]
        if len(commands) != 1:
            raise RuntimeError(
                "missing unique zstd verbose dry run; rerun prepare_corpus.py"
            )
        provenance = Path(commands[0]["log"])
        cwd = Path(project["source"]) / "lib"
        for line in provenance.read_text().splitlines():
            if "-c " not in line or source.name not in line:
                continue
            argv = shlex.split(line)
            if "-c" in argv and any(
                resolve(part, cwd) == source
                for part in argv
                if not part.startswith("-")
            ):
                candidates.append((argv, cwd, provenance))
    else:
        provenance = Path(project["prepared_manifest"])
        for entry in project["commands"]:
            argv, cwd = entry["command"], Path(entry["cwd"])
            if "-c" in argv and any(
                resolve(part, cwd) == source
                for part in argv
                if not part.startswith("-")
            ):
                candidates.append((argv, cwd, provenance))
    if len(candidates) != 1:
        raise RuntimeError(
            f"expected one compile command for {case['id']}, found {len(candidates)}"
        )
    return candidates[0]


def analysis_flags(argv: list[str], cwd: Path, source: Path) -> list[str]:
    """Drop only compilation outputs and dependency-generation options."""
    flags, index, sources = [], 1, 0
    while index < len(argv):
        arg = argv[index]
        if arg in ("-o", "-MF", "-MT", "-MQ"):
            if index + 1 == len(argv):
                raise RuntimeError(f"missing value after {arg}")
            index += 2
            continue
        if any(
            arg.startswith(prefix) and arg != prefix
            for prefix in ("-o", "-MF", "-MT", "-MQ")
        ):
            index += 1
            continue
        if arg in ("-c", "-MD", "-MMD", "-MP"):
            index += 1
            continue
        if not arg.startswith("-") and resolve(arg, cwd) == source:
            sources += 1
        else:
            flags.append(arg)
        index += 1
    if sources != 1 or any(arg.startswith("@") for arg in flags):
        raise RuntimeError(
            "compile command needs exactly one source and no response files"
        )
    return flags


def toucan_flags(flags: list[str], cwd: Path) -> dict:
    """Keep build include/define state; record every option outside the shipped profile."""
    includes, definitions, unmodeled = [], [], []
    language_mode = "gnu11"
    language_mode_source = "Toucan default; compiler default not inferred"
    index = 0
    while index < len(flags):
        arg = flags[index]
        if arg.startswith("-std="):
            mode = arg.removeprefix("-std=")
            if mode in ("c11", "gnu11"):
                language_mode = mode
                language_mode_source = arg
            else:
                unmodeled.append(arg)
                language_mode = "gnu11"
                language_mode_source = f"Toucan default; {arg} is not modeled"
            index += 1
            continue
        if arg in ("-I", "-D", "-U"):
            index += 1
            if index == len(flags):
                raise RuntimeError(f"missing value after {arg}")
            value = flags[index]
        elif arg[:2] in ("-I", "-D", "-U"):
            value, arg = arg[2:], arg[:2]
        else:
            if arg in (
                "-include",
                "-imacros",
                "-iquote",
                "-isystem",
                "-idirafter",
                "-nostdinc",
            ):
                raise RuntimeError(
                    f"build preprocessing option not represented by this audit: {arg}"
                )
            unmodeled.append(arg)
            index += 1
            continue
        if arg == "-I":
            includes.append(str(resolve(value, cwd)))
        elif arg == "-D":
            name, separator, replacement = value.partition("=")
            definitions.append([name, replacement if separator else "1"])
        else:
            definitions.append([value, None])
        index += 1
    return {
        "include_dirs": includes,
        "definitions": definitions,
        "unmodeled_compiler_flags": unmodeled,
        "language_mode": language_mode,
        "language_mode_source": language_mode_source,
    }


def dependency_paths(text: str, cwd: Path) -> list[Path]:
    """Decode GCC/Clang make escaping, including spaces, hashes and dollar signs."""
    text = text.replace("\\\n", "")
    if not text.startswith("audit:"):
        raise RuntimeError("unexpected compiler dependency target")
    text = text[len("audit:") :]
    words, current, index = [], [], 0
    while index < len(text):
        char = text[index]
        if char == "\\" and index + 1 < len(text):
            index += 1
            current.append(text[index])
        elif char == "$" and index + 1 < len(text) and text[index + 1] == "$":
            current.append("$")
            index += 1
        elif char.isspace():
            if current:
                words.append("".join(current))
                current = []
        else:
            current.append(char)
        index += 1
    if current:
        words.append("".join(current))
    return sorted({resolve(word, cwd) for word in words})


def hashes(paths: list[Path]) -> dict:
    return {str(path): digest(path) for path in sorted(set(paths))}


def verify_source_dependencies(project: dict, dependencies: dict, cache: Path) -> None:
    """Confirm used source-tree files still equal their pinned archive members."""
    root = Path(project["source"]).resolve()
    wanted = {
        f"{project['archive_root']}/{Path(path).relative_to(root).as_posix()}": (
            path,
            value,
        )
        for path, value in dependencies.items()
        if Path(path).is_relative_to(root)
    }
    if not wanted:
        return
    known = project.setdefault("verified_archive_members", {})
    missing = set(wanted) - known.keys()
    if missing:
        with tarfile.open(cache / project["archive"], "r:gz") as archive:
            for member in archive:
                if member.name not in missing:
                    continue
                stream = archive.extractfile(member)
                if stream is None:
                    raise RuntimeError(
                        f"source dependency is not an archive file: {member.name}"
                    )
                with stream:
                    known[member.name] = hashlib.file_digest(
                        stream, "sha256"
                    ).hexdigest()
                missing.remove(member.name)
        if missing:
            raise RuntimeError(
                f"source dependency is absent from pinned archive: {sorted(missing)}"
            )
    for member, (path, value) in wanted.items():
        if known[member] != value:
            raise RuntimeError(f"source dependency differs from pinned archive: {path}")


def probe(
    request: dict, args: argparse.Namespace, cwd: Path, directory: Path, stem: str
) -> dict:
    request_path = directory / f"{stem}.request.json"
    write_json(request_path, request)
    result = run(
        [str(args.probe), str(request_path)], cwd, directory, stem, args.timeout
    )
    output = Path(result["stdout"])
    if (
        result["timeout"]
        or result["exit_code"] not in (0, 1, 2)
        or output.stat().st_size > MAX_REPORT_BYTES
    ):
        result["result"] = {
            "status": "tool_error",
            "diagnostic": "probe failed, timed out, or exceeded its report limit",
        }
        return result
    try:
        result["result"] = json.loads(output.read_text())
    except (ValueError, OSError) as error:
        result["result"] = {
            "status": "tool_error",
            "diagnostic": f"invalid probe report: {error}",
        }
    payload = result["result"]
    if not isinstance(payload, dict) or payload.get("status") not in (
        "accepted",
        "preprocessed",
        "rejected",
        "tool_error",
    ):
        result["result"] = {
            "status": "tool_error",
            "diagnostic": "invalid probe status",
        }
    elif {"accepted": 0, "preprocessed": 0, "rejected": 1, "tool_error": 2}[
        payload["status"]
    ] != result["exit_code"]:
        result["result"] = {
            "status": "tool_error",
            "diagnostic": "probe status and exit code disagree",
        }
    elif payload["status"] == "rejected" and (
        payload.get("stage") not in ("analysis", "preprocessing")
        or not isinstance(payload.get("diagnostic"), str)
    ):
        result["result"] = {
            "status": "tool_error",
            "diagnostic": "missing rejection diagnostic",
        }
    if result["result"]["status"] == "accepted":
        path = Path(request["output"])
        result["declaration_sha256"] = digest(path)
        result["declaration_bytes"] = path.stat().st_size
    return result


def parity(normal: dict, retained: dict) -> bool:
    left, right = normal["result"], retained["result"]
    if left["status"] != right["status"] or left["status"] == "tool_error":
        return False
    if left["status"] == "accepted":
        return (
            normal.get("declaration_sha256") is not None
            and normal.get("declaration_sha256") == retained.get("declaration_sha256")
            and left["retained"] is False
            and right["retained"] is True
        )
    return (left.get("stage"), left.get("diagnostic")) == (
        right.get("stage"),
        right.get("diagnostic"),
    )


def audit(case: dict, project: dict, args: argparse.Namespace) -> dict:
    directory = args.output / case["id"]
    directory.mkdir()
    source = (Path(project[case["root"]]) / case["path"]).resolve()
    source_hash = digest(source)
    if "sha256" in case and source_hash != case["sha256"]:
        raise RuntimeError(f"pinned source was changed: {source}")
    command, cwd, provenance = compilation(project, case, source)
    flags = analysis_flags(command, cwd, source)
    cc = shutil.which(command[0])
    if cc is None:
        raise RuntimeError(f"compiler missing: {command[0]}")
    cc = str(Path(cc).resolve())
    result = {
        "id": case["id"],
        "project": case["project"],
        "source": str(source),
        "source_sha256": source_hash,
        "original_compile_command": command,
        "cwd": str(cwd),
        "command_provenance": {"path": str(provenance), "sha256": digest(provenance)},
        "compiler": {"path": cc, "sha256": digest(Path(cc))},
        "routes": {},
    }
    if "reference_generated_sha256" in case:
        result["matches_reference_generation"] = (
            source_hash == case["reference_generated_sha256"]
        )
    write_json(directory / "preparation.json", result)
    result["compiler"]["version"] = subprocess.check_output(
        [cc, "--version"], text=True
    ).strip()
    result["syntax"] = checked_run(
        [cc, *flags, "-fsyntax-only", str(source)],
        cwd,
        directory,
        "compiler-syntax",
        args.timeout,
    )
    depfile = directory / "compiler.d"
    result["dependency_command"] = checked_run(
        [cc, *flags, "-M", "-MF", str(depfile), "-MT", "audit", str(source)],
        cwd,
        directory,
        "compiler-dependencies",
        args.timeout,
    )
    dependencies = dependency_paths(depfile.read_text(), cwd)
    result["compiler_dependencies"] = hashes(dependencies)
    verify_source_dependencies(project, result["compiler_dependencies"], args.cache)
    native = checked_run(
        [cc, *flags, "-E", str(source)],
        cwd,
        directory,
        "compiler-preprocessed",
        args.timeout,
    )
    native_path = directory / "native.i"
    Path(native["stdout"]).rename(native_path)
    native["stdout"] = str(native_path)
    result["compiler_preprocessed"] = {
        **native,
        "sha256": digest(native_path),
        "bytes": native_path.stat().st_size,
    }
    search = checked_run(
        [cc, *flags, "-E", "-x", "c", "-v", "-"],
        cwd,
        directory,
        "compiler-includes",
        args.timeout,
    )
    text = Path(search["stderr"]).read_text()
    if (
        "#include <...> search starts here:" not in text
        or "End of search list." not in text
    ):
        raise RuntimeError("compiler did not report its include search list")
    search_paths = [
        str(resolve(line.strip(), cwd))
        for line in text.split("#include <...> search starts here:", 1)[1]
        .split("End of search list.", 1)[0]
        .splitlines()
        if line.strip()
    ]
    mapped = toucan_flags(flags, cwd)
    mapped["include_dirs"] = list(
        dict.fromkeys([*mapped["include_dirs"], *search_paths])
    )
    result["toucan_build_profile"] = mapped
    result["include_discovery"] = search
    for route, input_path, profile in [
        ("toucan", source, mapped),
        ("compiler_preprocessed", native_path, {"include_dirs": [], "definitions": []}),
    ]:
        route_dir = directory / route
        route_dir.mkdir()
        request = {
            "input": str(input_path),
            "target": args.target,
            "language_mode": mapped["language_mode"],
            "include_dirs": profile["include_dirs"],
            "definitions": profile["definitions"],
            "retain_code": False,
            **args.resource_limits,
        }
        preparation = probe(
            {
                **request,
                "operation": "preprocess",
                "output": str(route_dir / "toucan.i"),
            },
            args,
            cwd,
            route_dir,
            "preprocess",
        )
        entry = {"preprocessing": preparation}
        result["routes"][route] = entry
        if preparation["result"]["status"] == "preprocessed":
            paths = [Path(path) for path in preparation["result"]["dependencies"]]
            entry["dependency_sha256"] = hashes(paths)
            verify_source_dependencies(project, entry["dependency_sha256"], args.cache)
            embedded = preparation["result"].pop("embedded_headers")
            entry["embedded_header_sha256"] = {
                kind: {
                    name: hashlib.sha256(text.encode()).hexdigest()
                    for name, text in headers.items()
                }
                for kind, headers in embedded.items()
            }
            entry["preprocessed_sha256"] = digest(route_dir / "toucan.i")
        for retained in (False, True):
            name = "retained" if retained else "normal"
            entry[name] = probe(
                {
                    **request,
                    "operation": "analyze",
                    "retain_code": retained,
                    "output": str(route_dir / f"{name}.unit"),
                },
                args,
                cwd,
                route_dir,
                name,
            )
        entry["retention_parity"] = parity(entry["normal"], entry["retained"])
        entry["accepted"] = all(
            entry[name]["result"]["status"] == "accepted"
            for name in ("normal", "retained")
        )
        entry["completed_analysis"] = entry["accepted"] and entry["retention_parity"]
        print(
            case["id"],
            route,
            "accepted" if entry["accepted"] else "rejected",
            "parity=" + str(entry["retention_parity"]),
            flush=True,
        )
        if (
            "dependency_sha256" in entry
            and hashes([Path(path) for path in entry["dependency_sha256"]])
            != entry["dependency_sha256"]
        ):
            raise RuntimeError("Toucan dependency changed during analysis")
    if digest(source) != source_hash:
        raise RuntimeError("translation-unit source changed during audit")
    if hashes(dependencies) != result["compiler_dependencies"]:
        raise RuntimeError("compiler dependency changed during audit")
    write_json(directory / "evidence.json", result)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=ROOT / "corpus/cache")
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target")
    parser.add_argument(
        "--revision", required=True, help="Source revision used to build the probe"
    )
    parser.add_argument(
        "--build-metadata", type=Path, help="Optional immutable probe build record"
    )
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument(
        "--require",
        choices=("toucan", "both", "parity"),
        default="toucan",
        help="Required successes; other rejections remain explicit in the report",
    )
    args = parser.parse_args()
    if platform.system() != "Linux":
        parser.error(
            "the full-source build-command harness currently supports native Linux"
        )
    if args.timeout < 1:
        parser.error("--timeout must be positive")
    args.target = args.target or native_target()
    if args.target != native_target():
        parser.error("the prepared native build and audit target must match")
    args.cache, args.probe, args.output = (
        args.cache.resolve(),
        args.probe.resolve(),
        args.output.resolve(),
    )
    args.output.mkdir(parents=True, exist_ok=False)
    manifest = json.loads(MANIFEST.read_text())
    args.resource_limits = manifest["resource_limits"]
    prepared_path = args.cache / "prepared.json"
    prepared = json.loads(prepared_path.read_text())
    report = {
        "schema_version": 1,
        "purpose": "acceptance audit; failed partial analyses are not throughput samples",
        "created_utc": datetime.datetime.now(datetime.UTC).isoformat(),
        "target": args.target,
        "host": platform.platform(),
        "revision": args.revision,
        "probe": str(args.probe),
        "probe_sha256": digest(args.probe),
        "manifest_sha256": digest(MANIFEST),
        "prepared_manifest": str(prepared_path),
        "prepared_sha256": digest(prepared_path),
        "required": args.require,
        "resource_limits": args.resource_limits,
        "environment": {
            name: os.environ.get(name)
            for name in (
                "CPATH",
                "C_INCLUDE_PATH",
                "CFLAGS",
                "CPPFLAGS",
                "SDKROOT",
                "GCC_EXEC_PREFIX",
                "COMPILER_PATH",
                "LIBRARY_PATH",
            )
        },
        "fixed_environment": {"LC_ALL": "C", "SOURCE_DATE_EPOCH": "0"},
        "cases": [],
        "failures": [],
    }
    if args.build_metadata:
        build = json.loads(args.build_metadata.read_text())
        if build.get("probe_sha256") != report["probe_sha256"]:
            parser.error("probe hash disagrees with build metadata")
        report["probe_build"] = build
    projects = {
        p["name"]: {**p, "prepared_manifest": str(prepared_path)}
        for p in prepared["projects"]
    }
    report["project_preparation"] = {
        name: {
            "source": project["source"],
            "build": project["build"],
            "archive": {
                key: project[key]
                for key in (
                    "archive",
                    "archive_root",
                    "archive_ref",
                    "sha256",
                    "commit",
                    "version",
                )
            },
            "commands": [
                {**command, "log_sha256": digest(Path(command["log"]))}
                for command in project["commands"]
            ],
        }
        for name, project in projects.items()
    }
    report["script_sha256"] = {
        str(path.relative_to(ROOT)): digest(path)
        for path in [
            Path(__file__).resolve(),
            ROOT / "scripts/prepare_corpus.py",
            ROOT / "scripts/verify_corpus.py",
            ROOT / "crates/toucan/examples/audit_translation_unit.rs",
        ]
    }
    try:
        for name, pin in manifest["projects"].items():
            project = projects[name]
            if any(project[key] != value for key, value in pin.items()):
                raise RuntimeError(
                    f"prepared project does not match source pin: {name}"
                )
            if digest(args.cache / project["archive"]) != pin["sha256"]:
                raise RuntimeError(f"archive does not match source pin: {name}")
        for case in manifest["cases"]:
            try:
                report["cases"].append(audit(case, projects[case["project"]], args))
            except (
                OSError,
                ValueError,
                KeyError,
                RuntimeError,
                subprocess.SubprocessError,
            ) as error:
                report["failures"].append({"id": case["id"], "error": str(error)})
                print(case["id"], "ERROR", error, flush=True)
            write_json(args.output / "evidence.json", report)
        for case in report["cases"]:
            for route, entry in case["routes"].items():
                required = args.require == "both" or (
                    args.require == "toucan" and route == "toucan"
                )
                if (
                    not entry["retention_parity"]
                    or (required and not entry["accepted"])
                    or any(
                        entry[name]["result"]["status"] == "tool_error"
                        for name in ("normal", "retained")
                    )
                ):
                    report["failures"].append(
                        {
                            "id": case["id"],
                            "route": route,
                            "error": "required acceptance or retention parity failed",
                        }
                    )
        if digest(args.probe) != report["probe_sha256"]:
            report["failures"].append(
                {"error": "probe executable changed during audit"}
            )
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        report["failures"].append({"error": str(error)})
    report["summary"] = {
        "expected_translation_units": len(manifest["cases"]),
        "completed_by_route": {
            route: sum(
                case["routes"][route]["completed_analysis"] for case in report["cases"]
            )
            for route in ("toucan", "compiler_preprocessed")
        },
        "retention_pairs_agree": sum(
            entry["retention_parity"]
            for case in report["cases"]
            for entry in case["routes"].values()
        ),
    }
    report["compatibility_status"] = (
        "complete"
        if all(
            count == len(manifest["cases"])
            for count in report["summary"]["completed_by_route"].values()
        )
        else "has_rejections"
    )
    report["status"] = (
        "passed"
        if not report["failures"] and len(report["cases"]) == len(manifest["cases"])
        else "failed"
    )
    write_json(args.output / "evidence.json", report)
    print(args.output / "evidence.json")
    return int(report["status"] != "passed")


if __name__ == "__main__":
    raise SystemExit(main())
