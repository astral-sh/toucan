#!/usr/bin/env python3
"""Compare complete parsers, semantic analysis, and Builder generation across revisions."""

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


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline", type=Path, required=True, help="build.py output directory"
    )
    parser.add_argument(
        "--head", type=Path, required=True, help="build.py output directory"
    )
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pairs", type=int, default=7)
    parser.add_argument("--iterations", type=int, default=20)
    parser.add_argument("--rss-samples", type=int, default=3)
    parser.add_argument("--seed", type=int, default=20260912)
    parser.add_argument("--cpu", type=int, default=3)
    args = parser.parse_args()
    if args.pairs < 3 or args.iterations < 3 or args.rss_samples < 1:
        parser.error("require at least three pairs/iterations and one RSS sample")
    affinity = sorted(os.sched_getaffinity(0))
    if args.cpu not in affinity:
        parser.error("chosen CPU is outside allowed affinity")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    inputs = args.inputs.resolve()
    manifest = json.loads((inputs / "manifest.json").read_text())
    input_paths = {
        entry["name"]: inputs / Path(entry["input"]).name
        for entry in manifest["inputs"]
    }
    input_hashes = {name: digest(path) for name, path in input_paths.items()}
    for entry in manifest["inputs"]:
        if "sha256" in entry and entry["sha256"] != input_hashes[entry["name"]]:
            raise ValueError("input changed since preprocessing capture")
    builds = {
        name: json.loads((path / "build.json").read_text())
        for name, path in [("baseline", args.baseline), ("head", args.head)]
    }
    if builds["baseline"]["compiler"] != builds["head"]["compiler"]:
        raise ValueError("baseline and head compilers differ")
    if builds["baseline"]["registry_packages"] != builds["head"]["registry_packages"]:
        raise ValueError("baseline and head registry dependency versions differ")
    if builds["baseline"]["cargo"] != builds["head"]["cargo"]:
        raise ValueError("baseline and head Cargo commands differ")
    harness_sources = {
        revision: {
            path: checksum
            for path, checksum in build["harness_sha256"].items()
            if path.startswith("src/")
        }
        for revision, build in builds.items()
    }
    if harness_sources["baseline"] != harness_sources["head"]:
        raise ValueError("baseline and head harness Rust sources differ")
    harness_manifests = {}
    for revision, directory in [("baseline", args.baseline), ("head", args.head)]:
        path = directory / "harness" / "Cargo.toml"
        if digest(path) != builds[revision]["harness_sha256"]["Cargo.toml"]:
            raise ValueError("harness manifest changed after compilation")
        harness_manifests[revision] = path.read_text().replace(
            builds[revision]["source"], "<SOURCE>"
        )
    if harness_manifests["baseline"] != harness_manifests["head"]:
        raise ValueError("baseline and head harness configuration differs")
    ignored_environment = {"CARGO_HOME", "CARGO_TARGET_DIR", "CARGO_BUILD_BUILD_DIR"}
    compiler_environment = {
        revision: {
            key: value
            for key, value in build["environment"].items()
            if key not in ignored_environment
        }
        for revision, build in builds.items()
    }
    if compiler_environment["baseline"] != compiler_environment["head"]:
        raise ValueError("baseline and head compiler flags or trust settings differ")
    if builds["baseline"]["full_arena"] or not builds["head"]["full_arena"]:
        raise ValueError("expected an owned baseline and a full-arena head")
    binaries = {}
    for revision, build in builds.items():
        (output / f"{revision}-build.json").write_text(
            json.dumps(build, indent=2) + "\n"
        )
        binaries[revision] = {}
        for mode, binary in build["builds"].items():
            path = Path(binary["path"])
            if digest(path) != binary["sha256"]:
                raise ValueError(f"{revision}/{mode}: binary changed after build")
            binaries[revision][mode] = path
    report = {
        "status": "running",
        "baseline": builds["baseline"]["revision"],
        "head": builds["head"]["revision"],
        "compiler": builds["head"]["compiler"],
        "platform": platform.platform(),
        "cpu_model": next(
            line.split(":", 1)[1].strip()
            for line in Path("/proc/cpuinfo").read_text().splitlines()
            if line.startswith("model name")
        ),
        "cpu": args.cpu,
        "allowed_affinity": affinity,
        "pairs": args.pairs,
        "iterations": args.iterations,
        "rss_samples": args.rss_samples,
        "seed": args.seed,
        "runner_sha256": digest(Path(__file__)),
        "environment": {
            key: os.environ[key]
            for key in [
                "SOURCE_DATE_EPOCH",
                "TOUCAN_BENCH_CORPUS",
                "TOUCAN_BENCH_CLANG_INCLUDE",
            ]
            if key in os.environ
        },
        "input_manifest": manifest,
        "input_sha256": input_hashes,
        "stages": {},
    }
    dependencies = {}
    prefix = ["taskset", "--cpu-list", str(args.cpu)]

    def invoke(revision: str, engine: str, mode: str, source: str, name: str) -> dict:
        binary_mode = "allocations" if mode == "allocations" else "timing"
        path = output / name
        command = prefix + [
            str(binaries[revision][binary_mode]),
            mode,
            engine,
            source,
            str(args.iterations),
            str(path),
        ]
        if mode == "rss":
            command = ["/usr/bin/time", "-f", "%M", "-o", str(path)] + command
        process = subprocess.run(
            command, capture_output=True, text=True, timeout=180, check=False
        )
        if process.returncode:
            (output / f"{name}.stderr").write_text(process.stderr)
            raise RuntimeError(
                f"{revision}/{engine}/{mode} failed: {process.stderr[:2000]}"
            )
        row = json.loads(process.stdout)
        if row["allocation_counting"] != (mode == "allocations"):
            raise ValueError("incorrect allocation instrumentation")
        if row["full_arena"] != builds[revision]["full_arena"]:
            raise ValueError("binary does not match its visitor configuration")
        if mode == "capture":
            row["output_sha256"] = digest(path)
            if engine == "parser":
                row["spans_sha256"] = digest(path.with_suffix(".spans"))
            if engine == "builder":
                builder_report = json.loads(
                    path.with_suffix(".report.json").read_text()
                )
                builder_report.pop("timings", None)
                row["builder_report"] = builder_report
                for dependency in builder_report["dependencies"]:
                    dependency = Path(dependency)
                    if dependency.is_file():
                        checksum = digest(dependency)
                        previous = dependencies.setdefault(str(dependency), checksum)
                        if previous != checksum:
                            raise ValueError("Builder header changed during capture")
        elif mode == "rss":
            row["peak_rss_kib"] = int(path.read_text().strip())
        return row

    stages = []
    for workload, path in input_paths.items():
        stages.append(
            (
                f"parser/{workload}",
                str(path),
                [
                    ("baseline", "baseline", "parser"),
                    ("head", "head", "parser"),
                    ("lang-c-0.15.1", "baseline", "lang-c"),
                ],
            )
        )
        for engine in ["analyze", "retained"]:
            stages.append(
                (
                    f"{engine}/{workload}",
                    str(path),
                    [("baseline", "baseline", engine), ("head", "head", engine)],
                )
            )
        if "-adler32-" not in workload:
            stages.append(
                (
                    f"builder/{workload}",
                    workload,
                    [("baseline", "baseline", "builder"), ("head", "head", "builder")],
                )
            )

    # Capture and validate every stage before collecting any timing samples.
    for stage, source, participants in stages:
        captures = {}
        for label, revision, engine in participants:
            captures[label] = invoke(
                revision,
                engine,
                "capture",
                source,
                f"{stage.replace('/', '-')}-{label}.capture",
            )
        expected = captures["baseline"]["output_sha256"]
        if any(row["output_sha256"] != expected for row in captures.values()):
            raise ValueError(
                f"{stage}: output differs; inspect capture files before timing"
            )
        if (
            stage.startswith("parser/")
            and captures["baseline"]["spans_sha256"] != captures["head"]["spans_sha256"]
        ):
            raise ValueError(f"{stage}: concrete source spans differ")
        if (
            stage.startswith("builder/")
            and captures["baseline"]["builder_report"]
            != captures["head"]["builder_report"]
        ):
            raise ValueError(f"{stage}: generation reports differ")
        report["stages"][stage] = {"captures": captures}
        print(f"{stage}: output equality passed", flush=True)
    report["dependency_sha256"] = dependencies
    (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")

    randomizer = random.Random(args.seed)
    for stage, source, participants in stages:
        data = report["stages"][stage]
        memory = {}
        for label, revision, engine in participants:
            row = invoke(
                revision,
                engine,
                "allocations",
                source,
                f"{stage.replace('/', '-')}-{label}-allocations",
            )
            if row["measurements"]["retained_after_drop_bytes"] != 0:
                raise ValueError(
                    f"{stage}/{label}: allocations retained after result destruction"
                )
            row["rss_samples_kib"] = [
                invoke(
                    revision,
                    engine,
                    "rss",
                    source,
                    f"{stage.replace('/', '-')}-{label}-rss-{sample}",
                )["peak_rss_kib"]
                for sample in range(args.rss_samples)
            ]
            memory[label] = row
        rows = []
        for pair in range(args.pairs):
            order = list(participants)
            randomizer.shuffle(order)
            for position, (label, revision, engine) in enumerate(order):
                row = invoke(
                    revision,
                    engine,
                    "timing",
                    source,
                    f"{stage.replace('/', '-')}-{label}-timing",
                )
                measured = row["measurements"]
                row.update(label=label, pair=pair, order=position)
                row["median_operation_ns"] = statistics.median(measured["operation_ns"])
                row["median_drop_ns"] = statistics.median(measured["drop_ns"])
                row["median_total_ns"] = statistics.median(
                    operation + cleanup
                    for operation, cleanup in zip(
                        measured["operation_ns"], measured["drop_ns"], strict=True
                    )
                )
                rows.append(row)
        summary = {}
        for label, _, _ in participants:
            label_rows = [row for row in rows if row["label"] == label]
            values = [row["median_operation_ns"] for row in label_rows]
            baseline = [
                next(
                    row["median_operation_ns"]
                    for row in rows
                    if row["label"] == "baseline" and row["pair"] == item["pair"]
                )
                for item in label_rows
            ]
            ratios = [
                value / base for value, base in zip(values, baseline, strict=True)
            ]
            summary[label] = {
                "median_operation_ns": statistics.median(values),
                "operation_process_median_range_ns": [min(values), max(values)],
                "median_drop_ns": statistics.median(
                    row["median_drop_ns"] for row in label_rows
                ),
                "median_total_ns": statistics.median(
                    row["median_total_ns"] for row in label_rows
                ),
                "paired_operation_ratios_to_baseline": ratios,
                "median_operation_ratio_to_baseline": statistics.median(ratios),
                "operation_ratio_range_to_baseline": [min(ratios), max(ratios)],
                "median_peak_rss_kib": statistics.median(
                    memory[label]["rss_samples_kib"]
                ),
                **memory[label]["measurements"],
            }
        data.update(summary=summary, memory=memory, timing=rows)
        (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(
            f"{stage}: "
            + "; ".join(
                f"{label} {summary[label]['median_operation_ns'] / 1e6:.3f} ms, {summary[label]['allocations']} allocations, {summary[label]['peak_live_bytes']} bytes peak"
                for label, _, _ in participants
            ),
            flush=True,
        )
    for revision, build in builds.items():
        for mode, binary in build["builds"].items():
            if digest(binaries[revision][mode]) != binary["sha256"]:
                raise ValueError("benchmark binary changed during measurement")
    if {name: digest(path) for name, path in input_paths.items()} != input_hashes:
        raise ValueError("frozen parser input changed during measurement")
    if {name: digest(Path(name)) for name in dependencies} != dependencies:
        raise ValueError("Builder dependency changed during measurement")
    if digest(Path(__file__)) != report["runner_sha256"]:
        raise ValueError("measurement script changed during measurement")
    report["status"] = "passed"
    (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
