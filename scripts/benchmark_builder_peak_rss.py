#!/usr/bin/env python3
"""Measure Linux peak RSS for the paired, captured Builder requests."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import platform
import random
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def captured_text(capture: dict, name: str) -> str:
    artifact = capture["original_text_artifacts"][name]
    payload = artifact["text"].encode()
    if (
        len(payload) != artifact["bytes"]
        or hashlib.sha256(payload).hexdigest() != artifact["sha256"]
    ):
        raise ValueError(f"corrupt captured artifact: {name}")
    return artifact["text"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=5)
    parser.add_argument("--iterations", type=int, default=3)
    parser.add_argument("--timeout", type=int, default=120)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("peak RSS in KiB requires Linux /usr/bin/time")
    if args.pairs < 3 or args.iterations < 3 or args.timeout < 1:
        parser.error("require at least three pairs and iterations and a positive timeout")
    if args.output.exists():
        parser.error(f"output already exists: {args.output}")

    with gzip.open(args.capture, "rt", encoding="utf-8") as stream:
        capture = json.load(stream)
    binary = Path(capture["binary"]["path"])
    libclang = Path(capture["libclang"]["resolved_path"])
    if digest(binary) != capture["binary"]["sha256"]:
        raise ValueError("the captured benchmark binary changed")
    if digest(libclang) != capture["libclang"]["sha256"]:
        raise ValueError("the captured libclang changed")

    engines = ("toucan-builder", "bindgen")
    projects = ("zlib", "sqlite", "zstd", "libgit2")
    requests = {}
    references = {}
    for project in projects:
        prefix = f"timing/{project}.reference"
        requests[project] = captured_text(capture, prefix + ".request.json")
        references[project] = json.loads(captured_text(capture, prefix + ".reference.json"))
        reference = references[project]
        if json.loads(requests[project]) != reference["request"]:
            raise ValueError(f"{project}: reference request changed")
        for name, expected in reference["dependency_sha256"].items():
            if digest(Path(name)) != expected:
                raise ValueError(f"{project}: header dependency changed: {name}")
        for engine in engines:
            hashes = {row["output_sha256"] for row in reference["observations"][engine]}
            if len(hashes) != 1 or engine not in reference["configurations"]:
                raise ValueError(f"{project}: incomplete reference for {engine}")

    args.output.mkdir(parents=True)
    rows = []
    report = {
        "status": "incomplete",
        "method": (
            "Linux /usr/bin/time %M (maximum process RSS in KiB); fresh process "
            "per observation, three or more generated calls after one warmup, "
            "paired randomized engine order. Includes loaded libraries and process "
            "startup. Not a measure of retained heap or total allocations."
        ),
        "capture": str(args.capture.resolve()),
        "capture_sha256": digest(args.capture),
        "binary": {**capture["binary"], "verified": True},
        "libclang": {**capture["libclang"], "verified": True},
        "host": {
            "platform": platform.platform(),
            "python": platform.python_version(),
            "cpu_affinity": sorted(os.sched_getaffinity(0)),
        },
        "pairs": args.pairs,
        "iterations": args.iterations,
        "rows": rows,
    }
    randomizer = random.Random(20260909)
    try:
        with tempfile.TemporaryDirectory(prefix="toucan-peak-rss-") as workspace:
            temporary = Path(workspace)
            for project in projects:
                request_path = temporary / f"{project}.json"
                request_path.write_text(requests[project])
                reference = references[project]
                for pair in range(args.pairs):
                    order = list(engines)
                    randomizer.shuffle(order)
                    for engine in order:
                        output = temporary / f"{project}-{engine}-{pair}.rs"
                        memory = temporary / f"{project}-{engine}-{pair}.rss"
                        command = [
                            str(binary), engine, str(request_path), str(args.iterations), str(output)
                        ]
                        process = subprocess.run(
                            ["/usr/bin/time", "-f", "%M", "-o", str(memory), *command],
                            capture_output=True,
                            timeout=args.timeout,
                            check=True,
                            env={
                                **os.environ,
                                "LIBCLANG_PATH": str(Path(capture["libclang"]["path"]).parent),
                            },
                        )
                        measured = json.loads(process.stdout)
                        expected_hashes = {
                            item["output_sha256"] for item in reference["observations"][engine]
                        }
                        actual_hash = digest(output)
                        if (
                            measured["engine"] != engine
                            or measured["mode"] != "timing"
                            or measured["configuration"] != reference["configurations"][engine]
                            or len(measured["samples_ms"]) != args.iterations
                            or actual_hash not in expected_hashes
                        ):
                            raise ValueError(f"{project}/{engine}: output or configuration changed")
                        peak_kib = int(memory.read_text().strip())
                        if peak_kib <= 0:
                            raise ValueError(f"{project}/{engine}: invalid maximum RSS")
                        rows.append({
                            "project": project,
                            "pair": pair,
                            "engine": engine,
                            "peak_rss_kib": peak_kib,
                            "output_sha256": actual_hash,
                            "warmup_ms": measured["warmup_ms"],
                            "samples_ms": measured["samples_ms"],
                            "stderr": process.stderr.decode(errors="replace"),
                        })
                        output.unlink()

        if digest(binary) != capture["binary"]["sha256"]:
            raise ValueError("the benchmark binary changed during measurement")
        if digest(libclang) != capture["libclang"]["sha256"]:
            raise ValueError("libclang changed during measurement")
        for project, reference in references.items():
            for name, expected in reference["dependency_sha256"].items():
                if digest(Path(name)) != expected:
                    raise ValueError(
                        f"{project}: header dependency changed during measurement: {name}"
                    )

        report["projects"] = {}
        for project in projects:
            pairs = [
                {row["engine"]: row["peak_rss_kib"] for row in rows
                 if row["project"] == project and row["pair"] == pair}
                for pair in range(args.pairs)
            ]
            report["projects"][project] = {
                "median_peak_rss_kib": {
                    engine: statistics.median(pair[engine] for pair in pairs) for engine in engines
                },
                "paired_bindgen_over_toucan_ratios": [
                    pair["bindgen"] / pair["toucan-builder"] for pair in pairs
                ],
            }
            report["projects"][project]["median_paired_ratio"] = statistics.median(
                report["projects"][project]["paired_bindgen_over_toucan_ratios"]
            )
        report["input_integrity"] = (
            "Captured binary, libclang, request configuration, generated output, and "
            "all referenced header dependencies verified; binary, libclang, and "
            "header dependencies rechecked after the run."
        )
        report["status"] = "passed"
    finally:
        (args.output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
