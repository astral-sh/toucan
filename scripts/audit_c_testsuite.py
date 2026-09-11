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
import sys
import tarfile
import tempfile
import time
import urllib.request
from pathlib import Path, PurePosixPath

sys.path.insert(0, str(Path(__file__).resolve().parent))
from compiler_diagnostics import has_crash_diagnostic

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "corpus/conformance/c-testsuite.json"
MODES = {
    "c90": ["-std=c90"],
    "gnu90": ["-std=gnu90"],
    "c99": ["-std=c99"],
    "gnu99": ["-std=gnu99"],
    "c17": ["-std=c17"],
    "gnu17": ["-std=gnu17"],
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
        "environment_overrides": {"LC_ALL": "C", "SOURCE_DATE_EPOCH": "0"},
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
                env={**os.environ, **record["environment_overrides"]},
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
    record["crash_diagnostic"] = has_crash_diagnostic(
        stdout.read_text(errors="replace"), stderr.read_text(errors="replace")
    )
    return record


def accepted(result: dict) -> bool:
    return (
        result["exit_code"] == 0
        and not result["timeout"]
        and "start_error" not in result
        and not result.get("crash_diagnostic", False)
    )


def tool_failure(result: dict) -> bool:
    code = result["exit_code"]
    # Only an ordinary diagnostic exit can establish source rejection.
    return (
        result["timeout"]
        or result["crash_diagnostic"]
        or "start_error" in result
        or code not in (0, 1)
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


def native_definitions(args: argparse.Namespace) -> list[tuple[str, str | None]]:
    """Mirror only source-selection flags whose meaning the existing probe preserves."""
    flags = [*args.cc_arg, *getattr(args, f"{args.compiler}_arg")]
    definitions = []
    index = 0
    while index < len(flags):
        flag = flags[index]
        option = flag[:2]
        if option not in ["-I", "-D", "-U"]:
            raise RuntimeError(
                f"native-source route cannot translate compiler argument: {flag}"
            )
        value = flag[2:]
        if not value:
            index += 1
            if index == len(flags):
                raise RuntimeError(
                    f"missing value for native-source compiler argument: {flag}"
                )
            value = flags[index]
        if not value or "\n" in value or "\r" in value:
            raise RuntimeError(f"invalid native-source compiler argument: {flag}")
        if option == "-I":
            if not Path(value).is_absolute() or not Path(value).is_dir():
                raise RuntimeError(
                    "native-source -I arguments must name existing absolute directories"
                )
        elif option == "-D":
            name, separator, replacement = value.partition("=")
            definitions.append((name, replacement if separator else "1"))
        else:
            definitions.append((value, None))
        index += 1
    return definitions


def compiler_include_dirs(diagnostics: str) -> list[str]:
    """Preserve the driver's resolved search order; do not guess framework semantics."""
    paths = []
    section = None
    complete = False
    for line in diagnostics.splitlines():
        line = line.strip()
        if line == '#include "..." search starts here:':
            section = "quote"
        elif line == "#include <...> search starts here:":
            section = "angle"
        elif line == "End of search list.":
            complete = section == "angle"
            break
        elif section and line:
            if (
                section == "quote"
                or not Path(line).is_absolute()
                or not Path(line).is_dir()
            ):
                raise RuntimeError(
                    f"native-source route cannot preserve compiler include entry: {line}"
                )
            paths.append(line)
    if not complete or not paths:
        raise RuntimeError(
            "compiler did not report a supported nonempty include search list"
        )
    return paths


def compiler_definitions(source: str) -> dict[str, str]:
    definitions = {}
    for line in source.splitlines():
        if not line.startswith("#define "):
            raise RuntimeError("unexpected compiler macro dump line")
        fields = line[len("#define ") :].split(maxsplit=1)
        if not fields or fields[0] in definitions:
            raise RuntimeError("missing or duplicate compiler predefined macro")
        definitions[fields[0]] = fields[1] if len(fields) > 1 else ""
    if not definitions:
        raise RuntimeError("compiler macro dump is empty")
    return definitions


def macro_differences(compiler: dict, toucan: dict) -> dict:
    return {
        "compiler_only": {
            name: compiler[name] for name in sorted(compiler.keys() - toucan.keys())
        },
        "toucan_only": {
            name: toucan[name] for name in sorted(toucan.keys() - compiler.keys())
        },
        "different": {
            name: {"compiler": compiler[name], "toucan": toucan[name]}
            for name in sorted(compiler.keys() & toucan.keys())
            if compiler[name] != toucan[name]
        },
    }


def probe_request(
    args: argparse.Namespace, source: Path, output: Path, operation: str
) -> dict:
    return {
        "operation": operation,
        "input": str(source),
        "target": args.target,
        "compiler": args.compiler,
        "language_mode": args.dialect,
        "include_dirs": args.native_configuration["include_dirs"],
        "definitions": args.native_configuration["definitions"],
        "retain_code": False,
        "max_preprocessing_tokens": 2_000_000,
        "retention_nodes": 1_000_000,
        "retention_edges": 4_000_000,
        "retention_payload_bytes": 128 * 1024 * 1024,
        "output": str(output),
    }


def run_probe(
    args: argparse.Namespace, source: Path, directory: Path, operation: str
) -> tuple[dict, dict, Path]:
    stem = f"native-{operation}"
    output = directory / (
        "native.i" if operation == "preprocess" else "native-declarations.txt"
    )
    request = directory / f"{stem}-request.json"
    write_json(request, probe_request(args, source, output, operation))
    command = run([args.native_probe, str(request)], directory, stem, args.timeout)
    command["request_sha256"] = digest(request)
    try:
        result = json.loads(Path(command["stdout"]).read_text())
        expected = {"preprocessed": 0, "accepted": 0, "rejected": 1, "tool_error": 2}
        status = result.get("status")
        if status not in expected or command["exit_code"] != expected[status]:
            raise ValueError("probe status and exit code disagree")
        if status in ["preprocessed", "accepted"]:
            if status != ("preprocessed" if operation == "preprocess" else "accepted"):
                raise ValueError("probe returned the wrong operation result")
            dependencies = result.get("dependencies")
            if not isinstance(dependencies, list) or not all(
                isinstance(path, str) for path in dependencies
            ):
                raise ValueError("probe did not report header dependencies")
            if (
                len(dependencies) > 65_536
                or not output.is_file()
                or output.stat().st_size > 256 * 1024 * 1024
            ):
                raise ValueError(
                    "probe output or dependency count exceeds audit limits"
                )
            command["output"] = {
                "path": str(output),
                "sha256": digest(output),
                "bytes": output.stat().st_size,
            }
        if status == "rejected" and result.get("stage") not in [
            "preprocessing",
            "analysis",
        ]:
            raise ValueError("probe rejection has no recognized stage")
    except (OSError, ValueError, AttributeError, TypeError) as error:
        command["protocol_error"] = str(error)
        result = {"status": "tool_error", "diagnostic": str(error)}
    return command, result, output


def dependency_hashes(dependencies: list[str], directory: Path) -> dict[str, str]:
    result = {}
    for value in dependencies:
        path = Path(value)
        if not path.is_absolute():
            path = directory / path
        result[str(path.resolve(strict=True))] = digest(path)
    return result


def configure_native(args: argparse.Namespace, source: Path) -> dict:
    if args.target != native_target():
        raise RuntimeError(
            "native-source audit currently requires the native host target"
        )
    definitions = native_definitions(args)
    cc = getattr(args, args.compiler)
    flags = compiler_flags(args, args.compiler, args.dialect)
    search = run(
        [cc, *flags, "-E", "-v", "-x", "c", str(source)],
        args.output,
        "native-include-search",
        args.timeout,
    )
    if not accepted(search) or tool_failure(search):
        raise RuntimeError(
            "compiler include search discovery failed; see native-include-search.stderr"
        )
    include_dirs = compiler_include_dirs(Path(search["stderr"]).read_text())
    dump = run(
        [cc, *flags, "-dM", "-E", "-x", "c", str(source)],
        args.output,
        "native-compiler-definitions",
        args.timeout,
    )
    if not accepted(dump) or tool_failure(dump):
        raise RuntimeError("compiler predefined-macro discovery failed")
    macros = compiler_definitions(Path(dump["stdout"]).read_text())
    args.native_configuration = {
        "include_dirs": include_dirs,
        "definitions": definitions,
    }
    command, result, _ = run_probe(args, source, args.output, "preprocess")
    if (
        not accepted(command)
        or tool_failure(command)
        or result["status"] != "preprocessed"
    ):
        raise RuntimeError("native-source probe configuration control failed")
    if not isinstance(result.get("definitions"), dict) or not isinstance(
        result.get("embedded_headers"), dict
    ):
        raise TypeError(
            "native-source probe did not report its macro and resource-header configuration"
        )
    return {
        "target": args.target,
        "compiler": args.compiler,
        "language_mode": args.dialect,
        "include_dirs": include_dirs,
        "definitions": definitions,
        "include_search_command": search,
        "compiler_definitions_command": dump,
        "compiler_definitions": macros,
        "probe_control": command,
        "probe_configuration": result,
        "macro_differences": macro_differences(macros, result["definitions"]),
        "timestamp": {
            "unix_seconds": 0,
            "compiler_and_cli_environment": {"LC_ALL": "C", "SOURCE_DATE_EPOCH": "0"},
            "probe": "PreprocessingTimestamp::UNIX_EPOCH library default",
        },
        "policy": "Toucan keeps its shipped target/compiler/language profile and feature queries. Driver include paths and ordered caller -D/-U options are shared; compiler builtin macro values are recorded, not installed into Toucan. All include entries use the probe's include_dirs; system-header warning metadata is not reproduced.",
    }


def audit_native(source: Path, args: argparse.Namespace, directory: Path) -> dict:
    """Require original-source analysis and keep preprocessing failures distinct."""
    result = {"status": "tool_failure", "source_before": digest(source)}
    try:
        preprocess, prepared, _ = run_probe(args, source, directory, "preprocess")
        result.update(preprocess=preprocess, preprocessing_result=prepared)
        if tool_failure(preprocess) or prepared["status"] == "tool_error":
            return result
        if prepared["status"] == "rejected":
            result["status"] = "preprocessing_rejected"
            return result
        result["dependencies_before"] = dependency_hashes(
            prepared["dependencies"], directory
        )
        if (
            prepared["definitions"]
            != args.native_configuration["probe_configuration"]["definitions"]
        ):
            raise ValueError("probe macro configuration changed between cases")
        analysis, checked, _ = run_probe(args, source, directory, "analyze")
        result.update(analysis=analysis, analysis_result=checked)
        paths = set(prepared["dependencies"]) | set(checked.get("dependencies", []))
        result["dependencies_after"] = dependency_hashes(sorted(paths), directory)
        changed = [
            path
            for path, value in result["dependencies_before"].items()
            if result["dependencies_after"].get(path) != value
        ]
        result["source_after"] = digest(source)
        if result["source_before"] != result["source_after"]:
            changed.append(str(source))
        result["changed_dependencies"] = sorted(set(changed))
        if changed:
            result["status"] = "input_changed"
        elif tool_failure(analysis) or checked["status"] == "tool_error":
            result["status"] = "tool_failure"
        elif checked["status"] == "rejected":
            result["status"] = f"{checked['stage']}_rejected"
        elif set(result["dependencies_before"]) != set(
            dependency_hashes(checked["dependencies"], directory)
        ):
            raise ValueError(
                "native analysis read a different dependency set from preprocessing"
            )
        else:
            result["status"] = "accepted"
    except (OSError, ValueError, KeyError, TypeError) as error:
        result.update(status="tool_failure", diagnostic=str(error))
    return result


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
    if args.native_source:
        result["native_source"] = audit_native(source, args, directory)
        if result["source_sha256"] != result["native_source"]["source_before"]:
            result["native_source"]["status"] = "input_changed"
        result["native_source"]["difference"] = (
            result["eligible"] and result["native_source"]["status"] != "accepted"
        )
    write_json(directory / "result.json", result)
    return result


def summarize(cases: list[dict], changed_tools: list[str]) -> dict:
    """Classify both exploratory and pedantic-positive cases without an exception list."""
    strict = [
        case for case in cases if case["eligible"] and case["both_accept"]["strict_c11"]
    ]
    summary = {
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
    native = [case for case in cases if "native_source" in case]
    if native:
        summary["native_source"] = {
            "checked": len(native),
            "accepted": sum(
                case["native_source"]["status"] == "accepted" for case in native
            ),
            "eligible_count": sum(case["eligible"] for case in native),
            "strict_eligible_count": sum(
                case["eligible"] and case["both_accept"]["strict_c11"]
                for case in native
            ),
            "strict_accepted": sum(
                case["eligible"]
                and case["both_accept"]["strict_c11"]
                and case["native_source"]["status"] == "accepted"
                for case in native
            ),
            "differences": [
                case["name"] for case in native if case["native_source"]["difference"]
            ],
            "strict_differences": [
                case["name"]
                for case in native
                if case["native_source"]["difference"]
                and case["both_accept"]["strict_c11"]
            ],
            **{
                status: [
                    case["name"]
                    for case in native
                    if case["native_source"]["status"] == status
                ]
                for status in [
                    "preprocessing_rejected",
                    "analysis_rejected",
                    "tool_failure",
                    "input_changed",
                ]
            },
        }
    return summary


def audit_failed(
    summary: dict,
    *,
    fail_on_difference: bool = False,
    fail_on_strict_difference: bool = False,
) -> bool:
    """Keep infrastructure failures fatal under either acceptance policy."""
    native = summary.get("native_source", {})
    return bool(
        native.get("tool_failure")
        or native.get("input_changed")
        or (fail_on_difference and native.get("differences"))
        or (fail_on_strict_difference and native.get("strict_differences"))
        or summary["tool_failures"]
        or summary["oracle_pipeline_failures"]
        or summary["changed_tools"]
        or (fail_on_difference and summary["differences"])
        or (fail_on_strict_difference and summary["strict_differences"])
    )


def verify_native_inputs(cases: list[dict], output: Path) -> tuple[dict, list[str]]:
    """Recheck every source/read header and keep case sidecars synchronized."""
    dependency_values = {}
    changed_dependencies = set()
    for case in cases:
        inputs = {
            **case["native_source"].get("dependencies_before", {}),
            case["source"]: case["source_sha256"],
        }
        for path, value in inputs.items():
            if path in dependency_values and dependency_values[path] != value:
                changed_dependencies.add(path)
            dependency_values[path] = value
    for path, value in dependency_values.items():
        try:
            if digest(Path(path)) != value:
                changed_dependencies.add(path)
        except OSError:
            changed_dependencies.add(path)
    if changed_dependencies:
        for case in cases:
            inputs = set(case["native_source"].get("dependencies_before", {})) | {
                case["source"]
            }
            changed = changed_dependencies.intersection(inputs)
            if changed:
                case["native_source"]["status"] = "input_changed"
                case["native_source"]["difference"] = case["eligible"]
                case["native_source"]["changed_dependencies"] = sorted(
                    set(case["native_source"].get("changed_dependencies", [])) | changed
                )
                write_json(
                    output / "cases" / Path(case["name"]).stem / "result.json", case
                )
    return dependency_values, sorted(changed_dependencies)


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
        choices=[mode for mode in MODES if mode != "strict_c11"],
        default="c11",
        help="dialect for preprocessing and the acceptance comparison",
    )
    parser.add_argument(
        "--native-source",
        action="store_true",
        help="also require original-source acceptance through Toucan's native preprocessor (native host only)",
    )
    parser.add_argument(
        "--native-probe",
        default=str(ROOT / "target/release/examples/audit_translation_unit"),
        help="existing audit_translation_unit example used by --native-source",
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
    native_configuration = None
    if args.native_source:
        args.native_probe = executable(args.native_probe)
        probe_source = ROOT / "crates/toucan/examples/audit_translation_unit.rs"
        tools["native_probe"] = {
            "path": args.native_probe,
            "sha256": digest(Path(args.native_probe)),
            "protocol_source": {
                "path": str(probe_source),
                "sha256": digest(probe_source),
            },
            "provenance_note": "The protocol source identifies the checked-out example. Build provenance must independently connect the executable to its source tree.",
        }
        args.native_configuration = configure_native(args, probe)
        native_configuration = args.native_configuration
    report = {
        "schema_version": 2 if args.native_source else 1,
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
                "native_source",
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
        "native_source_configuration": native_configuration,
        "method": "Original sources are classified with GCC and Clang in each supported language mode and in pedantic C11. The chosen compiler's -E -P output is validated by that same compiler before the Toucan acceptance comparison, which selects the matching GNU or Clang compiler profile. Sources are never linked or executed; this is not an ABI, runtime, or invalid-source rejection conformance test."
        + (
            " The additional native-source route preprocesses and analyzes the untouched original source using Toucan's shipped profile and the captured compiler include search; it does not require identical macro environments or identical preprocessed text."
            if args.native_source
            else " Toucan's native preprocessing is not exercised by this route."
        ),
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
    if args.native_source:
        report["native_dependency_hashes"], report["changed_native_dependencies"] = (
            verify_native_inputs(report["cases"], args.output)
        )
    changed_tools = [
        name
        for name in tools
        if digest(Path(tools[name]["path"])) != tools[name]["sha256"]
    ]
    if digest(Path(__file__)) != report["runner_sha256"]:
        changed_tools.append("audit_driver")
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
    except (OSError, ValueError, TypeError, RuntimeError, tarfile.TarError) as error:
        raise SystemExit(f"audit failed: {error}") from error
