#!/usr/bin/env python3
"""Verify this recorded benchmark without a checkout, compiler, or network."""

import gzip
import hashlib
import json
import math
from pathlib import Path
import random
import statistics


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    root = Path(__file__).resolve().parent
    manifest = json.loads((root / "manifest.json").read_text())
    for name, expected in manifest["files"].items():
        data = (root / name).read_bytes()
        assert len(data) == expected["bytes"] and sha(data) == expected["sha256"], name
    capture = json.loads(gzip.decompress((root / "capture.json.gz").read_bytes()))
    for digest, text in capture["text_by_sha256"].items():
        assert sha(text.encode()) == digest
    for name, entry in capture["files"].items():
        data = capture["text_by_sha256"][entry["sha256"]].encode()
        assert len(data) == entry["bytes"], name

    def raw(name):
        return capture["text_by_sha256"][capture["files"][name]["sha256"]]

    def value(name):
        return json.loads(raw(name))

    source = value("manifest.json")
    for name, entry in capture["files"].items():
        if name.startswith("source/"):
            assert (
                source["source_sha256"][name.removeprefix("source/")] == entry["sha256"]
            )
    summary = json.loads((root / "summary.json").read_text())
    assert summary == value("timing-reviewed.json")
    assert (
        summary["commit"]
        == source["commit"]
        == "66c87396bbe03b22085339a87b8c350c90be280b"
    )
    preflight = value("preflight/report.json")
    assert preflight["status"] == "awaiting-review"
    assert preflight["source_unchanged"] and preflight["binary_unchanged"]
    prefix = Path(capture["original_capture_root"])
    for path in preflight["commands"]:
        result = value(str(Path(path).relative_to(prefix)))
        assert result["exit_code"] == 0, path
    pre_review = value("preflight-reviewed.json")
    audit = value("timing-audit.json")
    assert audit["exit_code"] == 0 and audit["source_before"] == audit["source_after"]
    assert audit["binaries_before"] == audit["binaries_after"]
    assert audit["review"] == value("timing-review.json")
    assert audit["review"]["compiler_work_idle"]
    assert audit["review"]["preflight_sha256"] == sha(
        raw("preflight/report.json").encode()
    )
    for name, digest in summary["artifacts"].items():
        assert digest == sha(raw(name).encode()), name
    timing = value("timing/evidence.json")
    assert (
        timing["status"] == "passed"
        and timing["pairs"] == 7
        and timing["iterations"] == 10
    )
    assert timing["affinity"] == [3] and timing["random_seed"] == 20260909
    assert timing["binary_sha256"] == summary["binary"]["sha256"]
    assert timing["binary_sha256"] == pre_review["binary"]["sha256"]
    assert list(timing["projects"]) == [
        n + ".reference" for n in ("zlib", "sqlite", "zstd", "libgit2")
    ]
    generator = random.Random(20260909)
    engines = ["toucan-builder", "bindgen"]
    samples = first_calls = 0
    for project, record in timing["projects"].items():
        name = project.removesuffix(".reference")
        reference = value(f"preflight/{name}/{name}.reference.json")
        assert record["request"] == reference["request"]
        assert record["dependency_sha256"] == reference["dependency_sha256"]
        rows = {(row["pair"], row["engine"]): row for row in record["rows"]}
        assert len(record["rows"]) == 14
        assert set(rows) == {(pair, engine) for pair in range(7) for engine in engines}
        for pair in range(7):
            order = list(engines)
            generator.shuffle(order)
            for index, engine in enumerate(order):
                row = rows[pair, engine]
                assert row["order_in_pair"] == index and row["mode"] == "timing"
                assert row["configuration"] == reference["configurations"][engine]
                path = str(Path(row["command"][-1]).relative_to(prefix))
                assert row["sha256"] == sha(raw(path).encode())
                assert {
                    r["output_sha256"] for r in reference["observations"][engine]
                } == {row["sha256"]}
                assert len(row["samples_ms"]) == 10
                assert all(
                    math.isfinite(n) and n > 0
                    for n in row["samples_ms"] + [row["warmup_ms"]]
                )
                samples += len(row["samples_ms"])
                first_calls += 1
        medians = {
            engine: [
                statistics.median(rows[pair, engine]["samples_ms"]) for pair in range(7)
            ]
            for engine in engines
        }
        first = {
            engine: [rows[pair, engine]["warmup_ms"] for pair in range(7)]
            for engine in engines
        }
        ratios = [
            medians["bindgen"][pair] / medians["toucan-builder"][pair]
            for pair in range(7)
        ]
        expected = {
            "median_ms": {
                engine: statistics.median(values) for engine, values in medians.items()
            },
            "process_median_range_ms": {
                engine: [min(values), max(values)] for engine, values in medians.items()
            },
            "median_bindgen_over_toucan": statistics.median(ratios),
            "bindgen_over_toucan_range": [min(ratios), max(ratios)],
            "median_first_call_ms": {
                engine: statistics.median(values) for engine, values in first.items()
            },
        }
        assert record["paired"]["process_medians_ms"] == medians
        assert record["paired"]["first_calls_ms"] == first
        assert record["paired"]["bindgen_over_toucan_ratios"] == ratios
        for key, val in expected.items():
            assert (
                record["paired"][key] == val and summary["projects"][name][key] == val
            )
    assert samples == summary["measured_calls"] == 560
    assert first_calls == summary["first_calls"] == 56
    print(
        f"Verified {len(capture['files'])} recorded text artifacts, {samples} samples, and {first_calls} first calls."
    )


if __name__ == "__main__":
    main()
