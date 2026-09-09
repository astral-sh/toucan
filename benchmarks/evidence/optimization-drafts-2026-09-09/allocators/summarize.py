#!/usr/bin/env python3
"""Summarize complete allocator captures without using allocation-counter data.

Run after the six allocator variants have completed preflight and timing:
    python3 summarize.py --json-output summary.json > summary.md
"""
import argparse
import hashlib
import json
import math
import statistics
import sys
from pathlib import Path

WORK = Path(__file__).resolve().parent
SOURCES = ("baseline", "optimized")
ALLOCATORS = ("system", "jemalloc", "mimalloc")
NAMES = [f"{source}-{allocator}" for source in SOURCES for allocator in ALLOCATORS]
WORKLOADS = [(project, "toucan-builder") for project in ("zlib", "sqlite", "zstd", "libgit2")]
WORKLOADS += [(project, "checked") for project in ("libgit2", "zlib", "zlib-adler32")]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def distribution(values):
    return {"median": statistics.median(values), "min": min(values), "max": max(values), "values": values}


def compare(candidate, reference, name):
    result = {"reference": name}
    for metric in ("latency_ms", "process_rss_kib"):
        current, previous = candidate[metric]["values"], reference[metric]["values"]
        require(len(current) == len(previous), "unpaired process observations")
        ratios = [value / earlier for value, earlier in zip(current, previous)]
        result[metric] = {
            "candidate_over_reference": distribution(ratios),
            "change_percent": distribution([(ratio - 1) * 100 for ratio in ratios]),
        }
    result["latency_ms"]["speedup_reference_over_candidate"] = distribution([
        earlier / value
        for value, earlier in zip(candidate["latency_ms"]["values"], reference["latency_ms"]["values"])
    ])
    return result


def load_capture(path, mode):
    capture = json.loads(path.read_text())
    require(capture.get("status") == "passed", f"{path}: incomplete capture")
    require(capture.get("mode") == mode, f"{path}: unexpected capture mode")
    variants = {item["name"]: item for item in capture["variants"]}
    require(len(capture["variants"]) == len(NAMES) and set(variants) == set(NAMES), f"{path}: require all six allocator variants")
    indexed = {}
    for row in capture["rows"]:
        key = (row["project"], row["engine"], row["variant"], row["round"])
        require(key not in indexed, f"{path}: duplicate process observation {key}")
        require(key[:2] in WORKLOADS and key[2] in variants, f"{path}: unexpected workload {key}")
        variant = variants[key[2]]
        require(row["allocator"] == variant["allocator"] and row["source_label"] == variant["source_label"], f"{path}: inconsistent allocator/source labels")
        require(not row["instrumented"], f"{path}: allocation-instrumented latency")
        require(isinstance(key[3], int) and key[3] >= 0, f"{path}: invalid round")
        samples = row["samples_ms"]
        require(len(samples) >= 3 and all(math.isfinite(value) and value > 0 for value in samples), f"{path}: invalid timing samples")
        require(len(row["allocations"]) == len(samples) and all(value is None for value in row["allocations"]), f"{path}: custom allocator must not use System counters")
        require(isinstance(row["peak_rss_kib"], int) and row["peak_rss_kib"] > 0, f"{path}: invalid process RSS")
        indexed[key] = row
    rounds = None
    for project, engine in WORKLOADS:
        for name in NAMES:
            found = sorted(key[3] for key in indexed if key[:3] == (project, engine, name))
            require(found and found == list(range(len(found))), f"{path}: missing rounds for {project}/{engine}/{name}")
            if rounds is None:
                rounds = found
            require(found == rounds, f"{path}: unmatched rounds")
    if mode == "preflight":
        require(rounds == [0], f"{path}: unexpected preflight rounds")
    return capture, variants, indexed, rounds


def validate_builds(variants, preflight_variants, build_root, primary_root):
    primary = json.loads((primary_root / "variants.json").read_text())
    require(primary[0]["name"] == "baseline" and len(primary) > 1, "missing primary source references")
    expected = {"baseline": primary[0], "optimized": primary[-1]}
    metadata_path = build_root.parent / "measurement-source.json"
    metadata = json.loads(metadata_path.read_text())["source_revisions"]
    receipt_path = build_root.parent / metadata["optimized"]["production_equivalence_receipt"]
    receipt = json.loads(receipt_path.read_text())
    require(receipt.get("status") == "passed", "production-source equivalence has not passed")
    require(receipt["primary_head"] == receipt["measured_parent"] == expected["optimized"]["head"], "unexpected allocator commit ancestry")
    require(receipt["primary_tree"] == expected["optimized"]["tree"], "primary production source tree changed")
    require(receipt["measured_head"] == metadata["optimized"]["head"] and receipt["measured_tree"] == metadata["optimized"]["tree"], "measured source differs from equivalence receipt")
    required_changes = {
        ("M", "benchmarks/inprocess/Cargo.lock"),
        ("M", "benchmarks/inprocess/Cargo.toml"),
        ("M", "benchmarks/inprocess/README.md"),
        ("A", "benchmarks/inprocess/src/allocator.rs"),
        ("M", "benchmarks/inprocess/src/main.rs"),
    }
    require(len(receipt["changed_paths"]) == len(required_changes) and {(item["status"], item["path"]) for item in receipt["changed_paths"]} == required_changes, "allocator commit changes production files")
    recorded_diff = receipt["diff_name_status"]
    require(hashlib.sha256(recorded_diff.encode()).hexdigest() == receipt["diff_sha256"], "source diff receipt hash changed")
    require({tuple(line.split("\t")) for line in recorded_diff.splitlines()} == required_changes, "source diff receipt disagrees with changed paths")
    require(set(receipt["unchanged_production_objects"]) == {"crates", "Cargo.toml", "Cargo.lock"}, "production objects missing from receipt")
    for path, item in receipt["unchanged_production_objects"].items():
        require(item["primary"] == item["measured"] and item["kind"] == ("tree" if path == "crates" else "blob"), f"production source/dependencies changed: {path}")
    for label in SOURCES:
        require(metadata[label]["primary_head"] == expected[label]["head"], f"source reference changed: {label}")
    require(metadata["baseline"]["head"] == expected["baseline"]["head"] and metadata["baseline"]["tree"] == expected["baseline"]["tree"], "baseline source changed")
    reference = primary_root / "build-inputs/baseline"
    reference_main = (reference / "main.rs").read_bytes()
    reference_counter = (reference / "counter.rs").read_bytes()
    reference_hashes = primary[0]["builds"][0]["driver_files"]
    require(digest(reference / "main.rs") == reference_hashes["main.rs"], "primary generation/checked functions changed")
    require(digest(reference / "counter.rs") == reference_hashes["counter.rs"], "primary counter source changed")
    allocator_hash = None
    group_locks = {}
    compiler = primary[0]["rustc"]
    for name in NAMES:
        variant = variants[name]
        source, allocator = name.split("-", 1)
        require(variant["source_label"] == source and variant["allocator"] == allocator, f"wrong labels: {name}")
        require(variant["head"] == metadata[source]["head"] and variant["tree"] == metadata[source]["tree"], f"unexpected frontend source: {name}")
        require(variant["rustc"] == compiler, f"compiler changed: {name}")
        require(variant["instrumented"] is False and len(variant["builds"]) == 1, f"unexpected build setup: {name}")
        for key in ("source_label", "allocator", "head", "tree", "rustc", "sha256", "builds"):
            require(variant[key] == preflight_variants[name][key], f"build changed since preflight: {name}/{key}")
        build = variant["builds"][0]
        files = build["driver_files"]
        frozen = build_root / name
        for filename, expected_hash in files.items():
            require(digest(frozen / filename) == expected_hash, f"frozen build input changed: {name}/{filename}")
        require((frozen / "main.rs").read_bytes() == reference_main + b"\nmod allocator;\n", f"benchmark function bodies changed: {name}")
        require((frozen / "counter.rs").read_bytes() == reference_counter, f"counter module changed: {name}")
        if allocator_hash is None:
            allocator_hash = files["allocator.rs"]
        require(files["allocator.rs"] == allocator_hash, f"allocator module changed: {name}")
        group_locks.setdefault(source, files["Cargo.lock"])
        require(files["Cargo.lock"] == group_locks[source], f"same-source dependency lock changed: {name}")
        command = build["command"]
        features = command[command.index("--features") + 1].split(",") if "--features" in command else []
        require(features == ([] if allocator == "system" else [f"allocator-{allocator}"]), f"unexpected allocator build features: {name}")
        require("--release" in command and "--locked" in command and "-Zohm-defaults=no" in command, f"unexpected compilation mode: {name}")
        for key, value in {"CARGO_PROFILE_RELEASE_LTO": "fat", "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "1", "CARGO_PROFILE_RELEASE_DEBUG": "0"}.items():
            require(build["build_environment"].get(key) == value, f"release profile changed: {name}/{key}")
    return {
        "primary_sources": {label: {key: item[key] for key in ("name", "head", "tree")} for label, item in expected.items()},
        "measured_sources": {label: {key: item[key] for key in ("head", "tree", "primary_head")} for label, item in metadata.items()},
        "production_source_equivalence": receipt,
        "source_metadata_sha256": digest(metadata_path),
        "source_equivalence_sha256": digest(receipt_path),
        "rustc": compiler,
        "reference_function_source_sha256": digest(reference / "main.rs"),
        "counter_source_sha256": digest(reference / "counter.rs"),
        "allocator_module_sha256": allocator_hash,
        "resolved_locks": group_locks,
        "functions_preserved": "Each frozen main.rs is exactly primary main.rs plus the allocator module declaration; counter.rs is byte-identical.",
    }


def summarize(capture_path, preflight_path, build_root, primary_root):
    timing, variants, timed, rounds = load_capture(capture_path, "timing")
    preflight, verified_variants, verified, _ = load_capture(preflight_path, "preflight")
    require(timing["inputs"] == preflight["inputs"], "allocator inputs changed after preflight")
    require(timing["platform"] == preflight["platform"], "allocator platform changed after preflight")
    provenance = validate_builds(variants, verified_variants, build_root, primary_root)
    primary_path = primary_root / "preflight/baseline/capture.json"
    primary = json.loads(primary_path.read_text())
    require(primary.get("status") == "passed", "primary baseline preflight is incomplete")
    require(all(timing["inputs"].get(path) == value for path, value in primary["inputs"].items()), "primary input hashes differ from allocator inputs")
    primary_rows = {(row["project"], row["engine"]): row for row in primary["rows"]}
    require(set(primary_rows) == set(WORKLOADS), "missing primary baseline workloads")
    workloads = []
    llvm_include_dirs = set()
    for project, engine in WORKLOADS:
        reference = primary_rows[(project, engine)]
        llvm_include_dirs.update(path for path in reference["configuration"]["include_dirs"] if "llvm" in path.lower())
        items = {}
        for name in NAMES:
            rows = [timed[(project, engine, name, index)] for index in rounds]
            for row in [*rows, verified[(project, engine, name, 0)]]:
                require(row["configuration"] == reference["configuration"], f"request configuration changed: {project}/{engine}/{name}")
                require(row["output_sha256"] == reference["output_sha256"], f"output changed: {project}/{engine}/{name}")
            items[name] = {
                "name": name,
                "source_label": variants[name]["source_label"],
                "allocator": variants[name]["allocator"],
                "latency_ms": distribution([statistics.median(row["samples_ms"]) for row in rows]),
                "process_rss_kib": distribution([row["peak_rss_kib"] for row in rows]),
                "samples_per_process": [len(row["samples_ms"]) for row in rows],
            }
        for source in SOURCES:
            reference_name = f"{source}-system"
            for allocator in ALLOCATORS[1:]:
                name = f"{source}-{allocator}"
                items[name]["vs_same_source_system"] = compare(items[name], items[reference_name], reference_name)
        frontend_changes = []
        for allocator in ALLOCATORS:
            baseline, optimized = f"baseline-{allocator}", f"optimized-{allocator}"
            frontend_changes.append({"allocator": allocator, "candidate": optimized, **compare(items[optimized], items[baseline], baseline)})
        workloads.append({"project": project, "engine": engine, "output_sha256": reference["output_sha256"], "variants": list(items.values()), "frontend_changes": frontend_changes})
    return {
        "status": "passed",
        "method": {
            "latency": "Median of uninstrumented process medians after one discarded warmup per process.",
            "pairing": "Ratios pair the same round and workload. Allocator comparisons use the same frontend revision; frontend comparisons hold the allocator fixed.",
            "ranges": "Observed minimum/maximum same-round ratios, not confidence intervals or significance tests. Negative changes mean less time or lower process RSS; speedup above 1 means faster.",
            "rss": "Whole-process peak RSS includes verification and checked-graph JSON serialization, not frontend-only peak or retained-heap memory.",
            "allocation_counts": "Not collected. Custom allocator timing must not be combined with the System allocation counter study.",
            "scope": "Local experiment; no CI or statistical-significance claim.",
        },
        "provenance": provenance,
        "llvm_resource_include_dirs": sorted(llvm_include_dirs),
        "platform": timing["platform"],
        "affinity": timing["affinity"],
        "rounds": rounds,
        "input_files": len(timing["inputs"]),
        "removed_allocator_environment": timing["removed_allocator_environment"],
        "sources": {str(path): digest(path) for path in (capture_path, preflight_path, primary_path)},
        "variants": [{key: variants[name][key] for key in ("name", "source_label", "allocator", "head", "tree", "sha256", "rustc")} for name in NAMES],
        "workloads": workloads,
    }


def change_text(comparison, metric):
    if comparison is None:
        return "—"
    values = comparison[metric]["change_percent"]
    return f"{values['median']:+.2f}% [{values['min']:+.2f}, {values['max']:+.2f}]"


def markdown(summary):
    lines = ["# Toucan allocator measurements", "",
             f"{len(summary['rounds'])} processes per variant/workload; {summary['input_files']} frozen input files.", "",
             "Timings are medians of process medians. Allocator changes compare the same frontend source against System. Brackets show observed paired-round ranges, not confidence intervals. Negative changes mean smaller values.", "",
             "RSS covers the entire process, including output verification and checked-graph JSON serialization. Allocation counts are not collected in this study. These local results make no CI or statistical-significance claim.", ""]
    for source, item in summary["provenance"]["measured_sources"].items():
        lines.append(f"- {source}: `{item['head']}`")
        if item["primary_head"] != item["head"]:
            lines.append(f"  Production crates and workspace dependencies match `{item['primary_head']}`; the measured commit adds only benchmark allocator selection.")
    lines += [""]
    for workload in summary["workloads"]:
        lines += [f"## {workload['project']} / {workload['engine']}", "",
                  "| Frontend | Allocator | Median ms | vs System % [range] | Process RSS MiB [range] | RSS vs System % [range] |",
                  "|---|---|---:|---:|---:|---:|"]
        for item in workload["variants"]:
            comparison = item.get("vs_same_source_system")
            rss = item["process_rss_kib"]
            lines.append(f"| {item['source_label']} | {item['allocator']} | {item['latency_ms']['median']:.3f} | {change_text(comparison, 'latency_ms')} | {rss['median']/1024:.2f} [{rss['min']/1024:.2f}, {rss['max']/1024:.2f}] | {change_text(comparison, 'process_rss_kib')} |")
        lines += ["", "Frontend changes from baseline to optimized with allocator held fixed:", "",
                  "| Allocator | Time change % [range] | Process RSS change % [range] |",
                  "|---|---:|---:|"]
        for item in workload["frontend_changes"]:
            lines.append(f"| {item['allocator']} | {change_text(item, 'latency_ms')} | {change_text(item, 'process_rss_kib')} |")
        lines += [""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", type=Path, default=WORK / "timing/all/capture.json")
    parser.add_argument("--preflight", type=Path, default=WORK / "preflight/all/capture.json")
    parser.add_argument("--build-inputs", type=Path, default=WORK / "build-inputs")
    parser.add_argument("--primary", type=Path, default=WORK.parent)
    parser.add_argument("--json-output", type=Path)
    parser.add_argument("--markdown-output", type=Path)
    args = parser.parse_args()
    try:
        summary = summarize(args.capture, args.preflight, args.build_inputs, args.primary)
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(2, f"Cannot summarize allocator captures: {error}\n")
    text = markdown(summary)
    if args.json_output:
        args.json_output.write_text(json.dumps(summary, indent=2) + "\n")
    if args.markdown_output:
        args.markdown_output.write_text(text + "\n")
    else:
        sys.stdout.write(text + "\n")


if __name__ == "__main__":
    main()
