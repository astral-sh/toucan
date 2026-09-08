#!/usr/bin/env python3
"""Measure header binding generation with recorded inputs and optional bindgen comparison."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import random
import re
import signal
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def execute(command: list[str], timeout: float) -> subprocess.CompletedProcess[bytes]:
    with subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=os.name == "posix",
    ) as process:
        try:
            stdout, stderr = process.communicate(timeout=timeout)
        except subprocess.TimeoutExpired as error:
            # Also stop the tool wrapped by /usr/bin/time, which inherits the
            # output pipes and otherwise could outlive the timeout.
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGKILL)
            else:
                process.kill()
            process.communicate()
            raise SystemExit(f"command exceeded {timeout}s: {command!r}") from error
        if process.returncode:
            raise SystemExit(
                f"command failed ({process.returncode}): {command!r}\n{stderr.decode(errors='replace')}"
            )
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)


def version(command: list[str], timeout: float) -> str:
    return execute(command, timeout).stdout.decode().strip()


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cpu_model(timeout: float) -> str:
    if platform.system() == "Linux":
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            key, separator, value = line.partition(":")
            if separator and key.strip() in {"model name", "Hardware"}:
                return value.strip()
    if platform.system() == "Darwin":
        return version(["sysctl", "-n", "machdep.cpu.brand_string"], timeout)
    return platform.processor()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("header", type=Path)
    parser.add_argument("--toucan", type=Path, default=Path("target/release/toucan"))
    parser.add_argument("--bindgen", type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--sysroot", type=Path)
    parser.add_argument("-I", "--include-dir", type=Path, action="append", default=[])
    parser.add_argument("--allowlist", action="append", default=[])
    parser.add_argument("--iterations", type=int, default=15)
    parser.add_argument(
        "--timeout", type=float, default=60, help="seconds allowed per command"
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.iterations < 3:
        parser.error("at least three measured iterations are required")
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("timeout must be finite and positive")
    header = args.header.resolve()
    commands = {
        "toucan": [
            str(args.toucan.resolve()),
            "bindgen",
            str(header),
            "--target",
            args.target,
        ]
    }
    for path in args.include_dir:
        commands["toucan"].extend(["-I", str(path.resolve())])
    if args.sysroot:
        commands["toucan"].extend(["--sysroot", str(args.sysroot.resolve())])
    for pattern in args.allowlist:
        commands["toucan"].extend(["--allowlist", pattern])
    if args.bindgen:
        command = [
            str(args.bindgen.resolve()),
            str(header),
            "--no-doc-comments",
            "--no-layout-tests",
            "--no-prepend-enum-name",
            "--default-macro-constant-type",
            "signed",
            "--formatter",
            "none",
        ]
        for pattern in args.allowlist:
            pattern = (
                re.escape(pattern[:-1]) + ".*"
                if pattern.endswith("*")
                else re.escape(pattern)
            )
            for category in ["type", "function", "var"]:
                command.extend([f"--allowlist-{category}", pattern])
        command.extend(["--", "-x", "c", "-std=c11", f"--target={args.target}"])
        if args.sysroot:
            command.append(f"--sysroot={args.sysroot.resolve()}")
        for path in args.include_dir:
            command.extend(["-I", str(path.resolve())])
        commands["bindgen"] = command

    observations = {name: [] for name in commands}
    warmups = {name: [] for name in commands}
    outputs: dict[str, str] = {}
    invocations = {}
    binary_hashes_before = {
        name: digest(Path(command[0])) for name, command in commands.items()
    }
    header_hash_before = digest(header)
    seed = 20260907
    order = list(commands)
    randomizer = random.Random(seed)
    with tempfile.TemporaryDirectory(prefix="toucan-benchmark-") as temporary:
        work = Path(temporary)
        # Discover dependencies before timing, then verify their contents again
        # after the run. Both tools still receive a discarded warmup below.
        report = work / "report.json"
        report_command = [*commands["toucan"], "--report", str(report)]
        execute(report_command, args.timeout)
        metadata = json.loads(report.read_text())
        dependencies = sorted(
            {header, *(Path(path).resolve() for path in metadata["dependencies"])}
        )
        dependency_hashes_before = {str(path): digest(path) for path in dependencies}
        for iteration in range(-1, args.iterations):
            randomizer.shuffle(order)
            for position, name in enumerate(order, start=1):
                command = commands[name].copy()
                time_file = work / "time.txt"
                if platform.system() == "Linux" and Path("/usr/bin/time").exists():
                    invocation = [
                        "/usr/bin/time",
                        "-f",
                        "%M",
                        "-o",
                        str(time_file),
                        *command,
                    ]
                else:
                    invocation = command
                invocations[name] = invocation
                started = time.perf_counter_ns()
                result = execute(invocation, args.timeout)
                elapsed = (time.perf_counter_ns() - started) / 1_000_000
                outputs[name] = result.stdout.decode()
                observation = {
                    "iteration": iteration + 1,
                    "order_in_iteration": position,
                    "wall_ms": elapsed,
                    "output_bytes": len(result.stdout),
                    "output_sha256": hashlib.sha256(result.stdout).hexdigest(),
                    "stderr": result.stderr.decode(errors="replace"),
                }
                if invocation != command:
                    observation["peak_rss_kib"] = int(time_file.read_text().strip())
                if iteration >= 0:
                    observations[name].append(observation)
                else:
                    warmups[name].append(observation)

    dependency_hashes_after = {str(path): digest(path) for path in dependencies}
    binary_hashes_after = {
        name: digest(Path(command[0])) for name, command in commands.items()
    }
    inputs_unchanged = (
        dependency_hashes_before == dependency_hashes_after
        and binary_hashes_before == binary_hashes_after
        and header_hash_before == dependency_hashes_before[str(header)]
    )

    summaries = {}
    for name, samples in observations.items():
        summaries[name] = {
            key: {
                "median": statistics.median(sample[key] for sample in samples),
                "minimum": min(sample[key] for sample in samples),
                "maximum": max(sample[key] for sample in samples),
            }
            for key in ["wall_ms", "output_bytes", "peak_rss_kib"]
            if key in samples[0]
        }
    function_sets = {
        name: sorted(
            {
                function
                for block in re.findall(
                    r'(?:unsafe\s+)?extern\s+"[^\"]+"\s*\{([^{}]*)\}', source
                )
                for function in re.findall(r"pub fn (?:r#)?(\w+)", block)
            }
        )
        for name, source in outputs.items()
    }
    result = {
        "schema_version": 2,
        "measured_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "commit": version(["git", "rev-parse", "HEAD"], args.timeout),
        "working_tree_dirty": bool(
            version(["git", "status", "--porcelain"], args.timeout)
        ),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "cpu_count": os.cpu_count(),
        "cpu_model": cpu_model(args.timeout),
        "allowed_cpus": sorted(os.sched_getaffinity(0))
        if hasattr(os, "sched_getaffinity")
        else None,
        "target": args.target,
        "rustc": version(["rustc", "--version"], args.timeout),
        "toucan": version([str(args.toucan.resolve()), "--version"], args.timeout),
        "bindgen": version([str(args.bindgen.resolve()), "--version"], args.timeout)
        if args.bindgen
        else None,
        "header": str(header),
        "header_sha256": header_hash_before,
        "commands": commands,
        "timed_invocations": invocations,
        "untimed_report_command": report_command,
        "binary_sha256": binary_hashes_before,
        "binary_sha256_after": binary_hashes_after,
        "dependency_sha256": dependency_hashes_before,
        "dependency_sha256_after": dependency_hashes_after,
        "inputs_unchanged": inputs_unchanged,
        "seed": seed,
        "warmup_runs": 1,
        "warmup_observations": warmups,
        "timeout_seconds": args.timeout,
        "iterations": args.iterations,
        "observations": observations,
        "summary": summaries,
        "function_sets": function_sets,
        "outputs_stable": {
            name: len(
                {sample["output_sha256"] for sample in [*warmups[name], *samples]}
            )
            == 1
            for name, samples in observations.items()
        },
        "function_sets_match": len({tuple(names) for names in function_sets.values()})
        == 1
        if args.bindgen
        else None,
        "toucan_report": metadata,
        "limitations": [
            "Warm filesystem cache; includes subprocess startup and output capture.",
            "Matching function names does not prove equivalent signatures or complete outputs.",
            "Toucan emits compile-time layout assertions; bindgen layout tests, doc comments, and rustfmt are disabled.",
            "Allocation counts are not measured. Non-Linux peak RSS is not measured.",
            "Phase timings come from a separate, untimed report invocation.",
            "Dependency hashes cover Toucan-reported headers, not bindgen-only resource headers or shared libraries.",
            "CPU affinity is recorded where available; the harness does not pin CPUs or control host load and frequency.",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    if not inputs_unchanged:
        raise SystemExit(
            f"benchmark inputs changed during the run; invalid results saved to {args.output}"
        )
    print(json.dumps(summaries, indent=2))


if __name__ == "__main__":
    main()
