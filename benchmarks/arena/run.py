#!/usr/bin/env python3
"""Collect paired expression AST measurements from separate timing/counting builds."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import random
import statistics
import subprocess
from pathlib import Path

ENGINES = ("owned", "arena", "arena-owned")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timing", type=Path, required=True)
    parser.add_argument("--allocations", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=7)
    parser.add_argument("--iterations", type=int, default=200)
    parser.add_argument("--seed", type=int, default=20260912)
    parser.add_argument("--cpu", type=int)
    parser.add_argument("--configuration", default="unspecified")
    args = parser.parse_args()
    if args.pairs < 3 or args.iterations < 10:
        parser.error("at least three pairs and ten iterations are required")
    timing = args.timing.resolve()
    allocations = args.allocations.resolve()
    affinity = (
        sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else []
    )
    cpu = args.cpu if args.cpu is not None else (affinity[0] if affinity else None)
    if cpu is not None and cpu not in affinity:
        parser.error("chosen CPU is outside the allowed affinity")
    prefix = ["taskset", "--cpu-list", str(cpu)] if cpu is not None else []
    cases = json.loads(subprocess.check_output([str(timing), "list"]))
    expected_hashes: dict[str, tuple[str, str]] = {}

    def invoke(binary: Path, engine: str, case: str, counting: bool) -> dict:
        command = prefix + [str(binary), engine, case, str(args.iterations)]
        row = json.loads(subprocess.check_output(command, timeout=120))
        if row["allocation_counting"] != counting:
            raise ValueError("wrong allocation instrumentation for binary")
        source_hash = digest(row.pop("source").encode())
        ast_hash = digest(row.pop("canonical_ast").encode())
        hashes = (source_hash, ast_hash)
        if expected_hashes.setdefault(case, hashes) != hashes:
            raise ValueError(f"{case}: input or AST changed across runs")
        row["source_sha256"] = source_hash
        row["ast_sha256"] = ast_hash
        return row

    report = {
        "status": "running",
        "configuration": args.configuration,
        "platform": platform.platform(),
        "cpu": cpu,
        "allowed_affinity": affinity,
        "cpu_model": next(
            (
                line.split(":", 1)[1].strip()
                for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")
            ),
            None,
        )
        if Path("/proc/cpuinfo").exists()
        else platform.processor(),
        "pairs": args.pairs,
        "iterations": args.iterations,
        "seed": args.seed,
        "binaries": {
            "timing": {"path": str(timing), "sha256": digest(timing.read_bytes())},
            "allocations": {
                "path": str(allocations),
                "sha256": digest(allocations.read_bytes()),
            },
        },
        "cases": {},
    }
    randomizer = random.Random(args.seed)
    for name, source in cases:
        rows = []
        memory = {engine: invoke(allocations, engine, name, True) for engine in ENGINES}
        for engine, row in memory.items():
            if row["measurements"]["retained_bytes"] != 0:
                raise ValueError(
                    f"{name}/{engine}: live allocations did not return to baseline"
                )
        for pair in range(args.pairs):
            order = list(ENGINES)
            randomizer.shuffle(order)
            for position, engine in enumerate(order):
                row = invoke(timing, engine, name, False)
                row.update(pair=pair, order=position)
                measured = row["measurements"]
                row["median_total_ns"] = statistics.median(
                    parse + cleanup
                    for parse, cleanup in zip(
                        measured["parse_ns"], measured["drop_ns"], strict=True
                    )
                )
                row["median_parse_ns"] = statistics.median(measured["parse_ns"])
                row["median_drop_ns"] = statistics.median(measured["drop_ns"])
                rows.append(row)
        process_medians = {
            engine: [
                next(
                    row["median_total_ns"]
                    for row in rows
                    if row["engine"] == engine and row["pair"] == pair
                )
                for pair in range(args.pairs)
            ]
            for engine in ENGINES
        }
        summary = {
            engine: {
                "median_total_ns": statistics.median(process_medians[engine]),
                "process_median_range_ns": [
                    min(process_medians[engine]),
                    max(process_medians[engine]),
                ],
                "median_parse_ns": statistics.median(
                    row["median_parse_ns"] for row in rows if row["engine"] == engine
                ),
                "median_drop_ns": statistics.median(
                    row["median_drop_ns"] for row in rows if row["engine"] == engine
                ),
                "median_ratio_to_owned": statistics.median(
                    value / baseline
                    for value, baseline in zip(
                        process_medians[engine], process_medians["owned"], strict=True
                    )
                ),
                **memory[engine]["measurements"],
            }
            for engine in ENGINES
        }
        report["cases"][name] = {
            "source": source,
            "summary": summary,
            "memory": memory,
            "timing": rows,
        }
        print(
            f"{name}: "
            + "; ".join(
                f"{engine} {summary[engine]['median_total_ns'] / 1000:.2f} us, {summary[engine]['allocations']} allocs, {summary[engine]['peak_live_bytes']} bytes peak"
                for engine in ENGINES
            ),
            flush=True,
        )
    for binary, name in [(timing, "timing"), (allocations, "allocations")]:
        if digest(binary.read_bytes()) != report["binaries"][name]["sha256"]:
            raise ValueError("benchmark binary changed while collecting results")
    report["status"] = "passed"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
