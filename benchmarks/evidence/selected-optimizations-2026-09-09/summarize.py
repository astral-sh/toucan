#!/usr/bin/env python3
"""Summarize complete draft captures; never combine partial measurements.

Usage: python3 summarize.py --json-output summary.json > summary.md
Only timing binaries contribute latency estimates. Allocation binaries contribute
allocation request/byte counts; their instrumented timings are deliberately unused.
"""
import argparse
import hashlib
import json
import math
import statistics
import sys
import tomllib
from pathlib import Path

WORK = Path(__file__).resolve().parent
WORKLOADS = [(name, "toucan-builder") for name in ("zlib", "sqlite", "zstd", "libgit2")]
WORKLOADS += [("libgit2", "checked"), ("zlib", "checked")]
WORKLOADS += [("zlib-adler32", "checked")]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_inventory(work):
    """Verify the frozen final inventory and its four documented request additions."""
    inventory_path = work / "canonical-inputs.json"
    inventory = json.loads(inventory_path.read_text())
    require(inventory.get("status") == "passed" and inventory.get("format") == "toucan-frozen-input-inventory-v1", "invalid canonical input inventory")
    source = inventory["source"]
    require(source["archived_path"] == "historical-timing/capture.json", "unexpected canonical inventory source")
    source_path = work / source["archived_path"]
    require(hashlib.sha256(source_path.read_bytes()).hexdigest() == source["sha256"], "historical timing provenance hash differs")
    historical_timing = json.loads(source_path.read_text())
    require(historical_timing.get("status") == source["status"] == "passed" and historical_timing.get("mode") == source["mode"] == "timing", "canonical inventory source is not passed timing")
    require(inventory["inputs"] == historical_timing["inputs"] and inventory["input_count"] == len(inventory["inputs"]) == 136, "canonical final input inventory differs")
    preflight = inventory["historical_preflight"]
    require(preflight["archived_path"] == "historical-baseline/capture.json", "unexpected historical preflight source")
    preflight_path = work / preflight["archived_path"]
    require(hashlib.sha256(preflight_path.read_bytes()).hexdigest() == preflight["sha256"], "historical preflight provenance hash differs")
    historical_preflight = json.loads(preflight_path.read_text())
    require(historical_preflight.get("status") == "passed" and historical_preflight.get("mode") == "preflight", "historical preflight is incomplete")
    previous = historical_preflight["inputs"]
    require(preflight["input_count"] == len(previous) == 132, "historical preflight inventory count differs")
    require(all(inventory["inputs"].get(path) == sha for path, sha in previous.items()), "historical preflight input changed or missing")
    additions = {path: sha for path, sha in inventory["inputs"].items() if path not in previous}
    expected = {f"/home/dev-user/.cache/toucan/builder-refresh-66c8739-2026-09-09/timing/{project}.reference.request.json" for project in ("zlib", "sqlite", "zstd", "libgit2")}
    require(set(additions) == expected and additions == inventory["additional_requests"], "canonical inventory must add exactly the four known request files")
    return inventory, hashlib.sha256(inventory_path.read_bytes()).hexdigest()


def read_capture(path, mode, names):
    capture = json.loads(path.read_text())
    require(capture.get("status") == "passed", f"{path}: capture is not complete")
    require(capture.get("mode") == mode, f"{path}: unexpected capture mode")
    variants = capture["variants"]
    require([item["name"] for item in variants] == names, f"{path}: incomplete or reordered draft stack")
    indexed = {}
    rounds = None
    for row in capture["rows"]:
        key = (row["project"], row["engine"], row["variant"], row["round"])
        require(key not in indexed, f"{path}: duplicate process observation {key}")
        require(key[:2] in WORKLOADS and key[2] in names, f"{path}: unexpected workload {key}")
        require(isinstance(key[3], int) and key[3] >= 0, f"{path}: invalid round")
        require(row["instrumented"] == (mode == "allocations"), f"{path}: wrong instrumentation")
        samples = row["samples_ms"]
        require(len(samples) >= 3 and all(math.isfinite(value) and value > 0 for value in samples), f"{path}: invalid timing samples")
        require(isinstance(row["peak_rss_kib"], int) and row["peak_rss_kib"] > 0, f"{path}: invalid process RSS")
        allocations = row["allocations"]
        require(len(allocations) == len(samples), f"{path}: unmatched allocation sample count")
        if mode == "allocations":
            require(all(isinstance(value, list) and len(value) == 2 and all(isinstance(n, int) and n >= 0 for n in value) for value in allocations), f"{path}: invalid allocation counters")
        else:
            require(all(value is None for value in allocations), f"{path}: instrumented timing samples")
        indexed[key] = row
    for project, engine in WORKLOADS:
        for name in names:
            observed = sorted(key[3] for key in indexed if key[:3] == (project, engine, name))
            require(observed and observed == list(range(len(observed))), f"{path}: missing/nonconsecutive rounds for {project}/{engine}/{name}")
            if rounds is None:
                rounds = observed
            require(observed == rounds, f"{path}: unmatched rounds for {project}/{engine}/{name}")
    return capture, indexed, rounds


def distribution(values):
    return {"median": statistics.median(values), "min": min(values), "max": max(values), "values": values}


def changes(candidate, reference):
    require(len(candidate) == len(reference), "unpaired comparison")
    require(all(value > 0 for value in reference), "zero reference prevents relative comparison")
    ratios = [current / previous for current, previous in zip(candidate, reference)]
    return {
        "candidate_over_reference": distribution(ratios),
        "change_percent": distribution([(ratio - 1) * 100 for ratio in ratios]),
    }


def compare(candidate, reference, name):
    result = {"reference": name}
    for metric in ("latency_ms", "allocation_requests", "requested_bytes", "process_rss_kib"):
        result[metric] = changes(candidate[metric]["values"], reference[metric]["values"])
    result["latency_ms"]["speedup_reference_over_candidate"] = distribution([
        previous / current
        for current, previous in zip(candidate["latency_ms"]["values"], reference["latency_ms"]["values"])
    ])
    return result


def summarize(timing_path, allocation_path, drafts_path):
    drafts = json.loads(drafts_path.read_text())
    require(len(drafts) == 1 and drafts[0]['name'] == 'selected', 'require the two-variant selected study')
    names = ["baseline", *[item["name"] for item in drafts]]
    require(len(names) == len(set(names)), "duplicate draft names")
    timing, timed, timing_rounds = read_capture(timing_path, "timing", names)
    allocation, counted, allocation_rounds = read_capture(allocation_path, "allocations", names)
    inventory_path = drafts_path.parent / "canonical-inputs.json"
    inventory, inventory_sha = validate_inventory(drafts_path.parent)
    require(timing["inputs"] == inventory["inputs"], "timing input inventory differs from the frozen historical final inventory")
    require(timing["inputs"] == allocation["inputs"], "timing/allocation input hashes differ")
    require(timing["canonical_input_metadata_sha256"] == allocation["canonical_input_metadata_sha256"] == inventory_sha, "capture canonical inventory provenance differs")
    require(timing["platform"] == allocation["platform"], "timing/allocation platforms differ")
    require(timing["affinity"] == allocation["affinity"], "timing/allocation CPU affinities differ")
    baseline_compiler = timing["variants"][0]["rustc"]
    baseline_driver = timing["variants"][0]["builds"][0]["driver_files"]
    for variant in timing["variants"]:
        require(variant["rustc"] == baseline_compiler, f"compiler changed: {variant['name']}")
        require(len(variant["builds"]) == 2, f"missing normal/instrumented build: {variant['name']}")
        for build in variant["builds"]:
            for source in ("main.rs", "counter.rs"):
                require(build["driver_files"][source] == baseline_driver[source], f"benchmark driver changed: {variant['name']}/{source}")
    selection_path = drafts_path.parent / "selection.json"
    selection = json.loads(selection_path.read_text())
    require(timing["variants"][0]["head"] == selection["baseline_expected_head"], "fresh baseline revision differs from selection policy")
    require(timing_rounds == list(range(selection["timing_rounds"])), "timing capture does not contain the requested round count")
    require(allocation_rounds == list(range(selection["allocation_rounds"])), "allocation capture does not contain the requested round count")
    dependency_path = drafts_path.parent / "dependency-policy.json"
    dependency_policy = json.loads(dependency_path.read_text())
    shared = {(item["name"], item["version"], item["source"]): item for item in dependency_policy["shared_registry_packages"]}
    additions = {(item["name"], item["version"], item["source"]): item for item in dependency_policy["allowed_selected_additions"]}
    preparation = json.loads((drafts_path.parent / "preparation.json").read_text())
    for variant in timing["variants"]:
        expected_packages = shared | (additions if variant["name"] == "selected" else {})
        for instrumented_index, build in enumerate(variant["builds"]):
            directory = variant["name"] + ("-allocations" if instrumented_index else "")
            frozen = drafts_path.parent / "build-inputs" / directory
            for filename, expected_hash in build["driver_files"].items():
                require(hashlib.sha256((frozen / filename).read_bytes()).hexdigest() == expected_hash, f"frozen build input changed: {directory}/{filename}")
            for filename in ("main.rs", "counter.rs"):
                require(build["driver_files"][filename] == preparation["historical_driver_hashes"][filename], f"historical benchmark function bytes changed: {directory}/{filename}")
            packages = tomllib.loads((frozen / "Cargo.lock").read_text())["package"]
            registry = {(item["name"], item["version"], item["source"]): item for item in packages if "source" in item}
            require(registry == expected_packages, f"shared registry dependencies changed: {directory}")
            require([key[1] for key in registry if key[0] == "smallvec"] == ["1.15.2"], "SmallVec baseline pin changed")
            require(not any(key[0] == "thin-vec" for key in registry), "excluded ThinVec dependency present")
    for actual, expected in zip(timing["variants"][1:], drafts):
        require(actual["head"] == expected["head"], f"draft revision mismatch: {actual['name']}")
    for left, right in zip(timing["variants"], allocation["variants"]):
        for key in ("head", "tree", "sha256", "alloc_sha256", "rustc"):
            require(left[key] == right[key], f"timing/allocation variant {key} mismatch: {left['name']}")
    results = []
    for project, engine in WORKLOADS:
        baseline = timed[(project, engine, "baseline", 0)]
        reference_configuration = baseline["configuration"]
        reference_output = baseline["output_sha256"]
        items = {}
        for name in names:
            times = [timed[(project, engine, name, index)] for index in timing_rounds]
            counts = [counted[(project, engine, name, index)] for index in allocation_rounds]
            for row in times + counts:
                require(row["configuration"] == reference_configuration, f"configuration changed: {project}/{engine}/{name}")
                require(row["output_sha256"] == reference_output, f"output changed: {project}/{engine}/{name}")
            items[name] = {
                "name": name,
                "latency_ms": distribution([statistics.median(row["samples_ms"]) for row in times]),
                "allocation_requests": distribution([statistics.median([sample[0] for sample in row["allocations"]]) for row in counts]),
                "requested_bytes": distribution([statistics.median([sample[1] for sample in row["allocations"]]) for row in counts]),
                "process_rss_kib": distribution([row["peak_rss_kib"] for row in times]),
                "instrumented_process_rss_kib": distribution([row["peak_rss_kib"] for row in counts]),
                "timing_samples_per_process": [len(row["samples_ms"]) for row in times],
                "allocation_samples_per_process": [len(row["samples_ms"]) for row in counts],
            }
        for index, name in enumerate(names):
            if index:
                parent = names[index - 1]
                items[name]["vs_parent"] = compare(items[name], items[parent], parent)
                items[name]["vs_baseline"] = compare(items[name], items["baseline"], "baseline")
        results.append({"project": project, "engine": engine, "output_sha256": reference_output, "variants": list(items.values())})
    return {
        "status": "passed",
        "method": {
            "latency": "Median of uninstrumented process medians; one discarded warmup per process.",
            "comparisons": "Candidate/reference ratios pair process medians from the same round and workload. Negative percentage changes mean less time, fewer requests/bytes, or less process RSS. Speedup is reference/candidate; above 1 means faster.",
            "parent": "Selected contains the six authorized optimizations; its only comparison is the fresh baseline on the same main revision.",
            "ranges": "Minimum and maximum observed same-round ratios; these are not confidence intervals or significance tests.",
            "allocations": "Median of instrumented process medians. Counts include alloc, alloc_zeroed and realloc requests; bytes sum requested allocation sizes, not live or peak heap usage.",
            "rss": "Median whole-process peak RSS across timing processes, including output verification and checked-graph JSON serialization. Instrumented-process RSS is retained separately; neither measures frontend-only peak or retained heap memory.",
        },
        "sources": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in (timing_path, allocation_path, drafts_path, selection_path, dependency_path, inventory_path)},
        "dependency_validation": {"status": "passed", "shared_registry_packages": len(shared), "selected_registry_additions": len(additions), "smallvec": "1.15.2"},
        "platform": timing["platform"],
        "affinity": timing["affinity"],
        "input_files": len(timing["inputs"]),
        "timing_rounds": timing_rounds,
        "allocation_rounds": allocation_rounds,
        "variants": [{key: item[key] for key in ("name", "head", "tree", "sha256", "alloc_sha256", "rustc")} for item in timing["variants"]],
        "workloads": results,
    }


def percent(item, comparison, metric, include_range=False):
    if comparison not in item:
        return "—"
    values = item[comparison][metric]["change_percent"]
    text = f"{values['median']:+.2f}%"
    if include_range:
        text += f" [{values['min']:+.2f}, {values['max']:+.2f}]"
    return text


def markdown(summary):
    lines = ["# Toucan selected optimization measurements", "",
             f"Complete captures: {len(summary['timing_rounds'])} timing processes and {len(summary['allocation_rounds'])} allocation processes per variant/workload; {summary['input_files']} frozen input files.", "",
             "Timings are medians of process medians. Changes pair the same round; brackets show the observed minimum and maximum percentage changes, not confidence intervals. Negative changes mean smaller values. The selected variant contains only the six authorized optimizations; both comparisons use the fresh baseline.", "",
             "RSS is the whole benchmark process, including verification and checked-graph JSON serialization. Requested allocation bytes are cumulative requests, including reallocations; they are not live or peak heap usage.", ""]
    for workload in summary["workloads"]:
        lines += [f"## {workload['project']} / {workload['engine']}", "",
                  "| Draft | Median ms | vs parent % [range] | vs baseline % [range] | Process RSS MiB |",
                  "|---|---:|---:|---:|---:|"]
        for item in workload["variants"]:
            rss = item["process_rss_kib"]
            lines.append(f"| {item['name']} | {item['latency_ms']['median']:.3f} | {percent(item, 'vs_parent', 'latency_ms', True)} | {percent(item, 'vs_baseline', 'latency_ms', True)} | {rss['median'] / 1024:.2f} [{rss['min'] / 1024:.2f}, {rss['max'] / 1024:.2f}] |")
        lines += ["", "| Draft | Allocation requests | vs parent | vs baseline | Requested MiB | vs parent | vs baseline |",
                  "|---|---:|---:|---:|---:|---:|---:|"]
        for item in workload["variants"]:
            lines.append(f"| {item['name']} | {item['allocation_requests']['median']:,.0f} | {percent(item, 'vs_parent', 'allocation_requests')} | {percent(item, 'vs_baseline', 'allocation_requests')} | {item['requested_bytes']['median'] / 1048576:.3f} | {percent(item, 'vs_parent', 'requested_bytes')} | {percent(item, 'vs_baseline', 'requested_bytes')} |")
        lines += [""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timing", type=Path, default=WORK / "timing/all/capture.json")
    parser.add_argument("--allocations", type=Path, default=WORK / "allocations/all/capture.json")
    parser.add_argument("--drafts", type=Path, default=WORK / "drafts.json")
    parser.add_argument("--json-output", type=Path)
    parser.add_argument("--markdown-output", type=Path)
    args = parser.parse_args()
    try:
        result = summarize(args.timing, args.allocations, args.drafts)
    except (ValueError, KeyError, TypeError, OSError) as error:
        parser.exit(2, f"Cannot summarize captures: {error}\n")
    text = markdown(result)
    if args.json_output:
        args.json_output.write_text(json.dumps(result, indent=2) + "\n")
    if args.markdown_output:
        args.markdown_output.write_text(text + "\n")
    else:
        sys.stdout.write(text + "\n")


if __name__ == "__main__":
    main()
