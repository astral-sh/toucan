#!/usr/bin/env python3
"""Compare repeated library calls using saved CLI benchmark configurations."""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import random
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from benchmark import cpu_model, digest, execute


def request_from_reference(reference):
    """Translate the saved CLI configuration without broadening its allowlist."""
    if "request" in reference:
        return reference["request"]
    toucan = reference["commands"]["toucan"]
    bindgen = reference["commands"]["bindgen"]

    def values(command, flag):
        return [command[i + 1] for i, value in enumerate(command) if value == flag]

    return {
        "header": toucan[2],
        "target": values(toucan, "--target")[0],
        "sysroot": values(toucan, "--sysroot")[0],
        "include_dirs": values(toucan, "-I"),
        "allowlist": values(toucan, "--allowlist"),
        "bindgen_allowlist": values(bindgen, "--allowlist-type"),
    }


def paired_summary(rows, engines, pairs):
    """Compare process medians within each randomized pair, retaining first calls."""
    by_pair = {}
    for row in rows:
        key = (row["pair"], row["engine"])
        if key in by_pair:
            raise ValueError("duplicate benchmark process in a pair")
        by_pair[key] = row
    if set(by_pair) != {(pair, engine) for pair in range(pairs) for engine in engines}:
        raise ValueError("benchmark process pairs are incomplete")
    medians = {
        engine: [
            statistics.median(by_pair[pair, engine]["samples_ms"])
            for pair in range(pairs)
        ]
        for engine in engines
    }
    first_calls = {
        engine: [by_pair[pair, engine]["warmup_ms"] for pair in range(pairs)]
        for engine in engines
    }
    ratios = [
        medians["bindgen"][pair] / medians[engines[0]][pair] for pair in range(pairs)
    ]
    return {
        "method": "Median of process medians; speedup is the median of within-pair bindgen/Toucan ratios. First calls include initial library loading but exclude process startup.",
        "process_medians_ms": medians,
        "median_ms": {
            engine: statistics.median(values) for engine, values in medians.items()
        },
        "process_median_range_ms": {
            engine: [min(values), max(values)] for engine, values in medians.items()
        },
        "bindgen_over_toucan_ratios": ratios,
        "median_bindgen_over_toucan": statistics.median(ratios),
        "bindgen_over_toucan_range": [min(ratios), max(ratios)],
        "first_calls_ms": first_calls,
        "median_first_call_ms": {
            engine: statistics.median(values) for engine, values in first_calls.items()
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("references", nargs="+", type=Path)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument(
        "--toucan-engine", choices=("toucan", "toucan-builder"), default="toucan"
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=3)
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--seed", type=int, default=20260908)
    args = parser.parse_args()
    if args.pairs < 3 or args.iterations < 3:
        parser.error("at least three process pairs and three iterations are required")
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("timeout must be finite and positive")
    if len({path.stem for path in args.references}) != len(args.references):
        parser.error("reference filenames must have distinct stems")
    args.output.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve()
    randomizer = random.Random(args.seed)
    engines = (args.toucan_engine, "bindgen")
    report = {
        "status": "failed",
        "qualification": "Repeated library calls with one discarded warmup per process. Configuration, parsing, and string emission included; process startup and first libclang initialization excluded. Reference output hashes must match for each engine independently.",
        "platform": platform.platform(),
        "cpu_model": cpu_model(args.timeout),
        "affinity": sorted(os.sched_getaffinity(0))
        if hasattr(os, "sched_getaffinity")
        else None,
        "pairs": args.pairs,
        "iterations": args.iterations,
        "random_seed": args.seed,
        "toucan_engine": args.toucan_engine,
        "binary_sha256": digest(binary),
        "projects": {},
    }
    try:
        for path in args.references:
            reference_bytes = path.read_bytes()
            reference = json.loads(reference_bytes)
            project = path.stem
            (args.output / f"{project}.reference.json").write_bytes(reference_bytes)
            request = request_from_reference(reference)
            configurations = reference.get("configurations")
            if args.toucan_engine == "toucan-builder" and (
                request.get("policy") != "builder"
                or not request.get("allowlist_files")
                or request.get("allowlist")
                or request.get("bindgen_allowlist")
                or not configurations
                or any(engine not in configurations for engine in engines)
            ):
                raise ValueError(
                    f"{project}: Builder timing requires file roots, no legacy name filters, and captured configurations"
                )
            request_path = (args.output / f"{project}.request.json").resolve()
            request_path.write_text(json.dumps(request, indent=2) + "\n")
            dependencies = reference["dependency_sha256"]
            if dependencies != {name: digest(Path(name)) for name in dependencies}:
                raise ValueError(
                    f"{project}: header dependencies changed from reference"
                )
            expected = {
                engine: {
                    row["output_sha256"] for row in reference["observations"][engine]
                }
                for engine in engines
            }
            if any(len(hashes) != 1 for hashes in expected.values()):
                raise ValueError(f"{project}: reference outputs were not stable")
            rows = []
            result = {
                "request": request,
                "rows": rows,
                "dependency_sha256": dependencies,
            }
            report["projects"][project] = result
            for pair in range(args.pairs):
                order_in_pair = list(engines)
                randomizer.shuffle(order_in_pair)
                for order, engine in enumerate(order_in_pair):
                    output = (args.output / f"{project}-{engine}-{pair}.rs").resolve()
                    command = [
                        str(binary),
                        engine,
                        str(request_path),
                        str(args.iterations),
                        str(output),
                    ]
                    process = execute(command, args.timeout)
                    row = json.loads(process.stdout)
                    row.update(
                        pair=pair,
                        order_in_pair=order,
                        command=command,
                        stderr=process.stderr.decode(errors="replace"),
                        sha256=digest(output),
                    )
                    rows.append(row)
                    if (
                        row["engine"] != engine
                        or len(row["samples_ms"]) != args.iterations
                        or not math.isfinite(row["warmup_ms"])
                        or row["warmup_ms"] <= 0
                        or any(
                            not math.isfinite(value) or value <= 0
                            for value in row["samples_ms"]
                        )
                    ):
                        raise ValueError(
                            f"{project}: driver response does not match request"
                        )
                    if {row["sha256"]} != expected[engine]:
                        raise ValueError(
                            f"{project}: {engine} output changed from reference"
                        )
                    if (
                        configurations
                        and row.get("configuration") != configurations[engine]
                    ):
                        raise ValueError(
                            f"{project}: {engine} configuration changed from reference"
                        )
            if dependencies != {name: digest(Path(name)) for name in dependencies}:
                raise ValueError(
                    f"{project}: header dependencies changed during measurement"
                )
            medians = {
                engine: statistics.median(
                    value
                    for row in rows
                    if row["engine"] == engine
                    for value in row["samples_ms"]
                )
                for engine in engines
            }
            result.update(
                median_ms=medians,
                bindgen_over_toucan=medians["bindgen"] / medians[args.toucan_engine],
                paired=paired_summary(rows, engines, args.pairs),
            )
            print(project, result["paired"]["median_ms"], flush=True)
        if digest(binary) != report["binary_sha256"]:
            raise ValueError("benchmark executable changed during measurement")
        report["status"] = "passed"
    finally:
        (args.output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
