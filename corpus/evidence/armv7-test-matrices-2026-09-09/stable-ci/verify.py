"""Verify the portable CI record without network access or build tools."""

from __future__ import annotations

import gzip
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def compressed(record: dict) -> bytes:
    path = ROOT / record["path"]
    require(path.resolve().is_relative_to(ROOT), "Escaping evidence path")
    data = path.read_bytes()
    require(len(data) == record["bytes"], f"Compressed size: {path}")
    require(sha256(data) == record["sha256"], f"Compressed digest: {path}")
    raw = gzip.decompress(data)
    require(len(raw) == record["uncompressed_bytes"], f"Raw size: {path}")
    require(sha256(raw) == record["uncompressed_sha256"], f"Raw digest: {path}")
    return raw


def main() -> None:
    manifest = json.loads((ROOT / "manifest.json").read_text())
    actual = {p.relative_to(ROOT).as_posix() for p in ROOT.rglob("*") if p.is_file()}
    require(actual == set(manifest["files"]) | {"manifest.json"}, "Manifest file set")
    require(
        not any(Path(p).name == "Cargo.toml" for p in actual), "Live Cargo manifest"
    )
    for path, record in manifest["files"].items():
        data = (ROOT / path).read_bytes()
        require(
            len(data) == record["bytes"] and sha256(data) == record["sha256"],
            f"File digest: {path}",
        )
    summary = json.loads((ROOT / "summary.json").read_text())
    require(summary["schema_version"] == 1, "Schema version")
    source = summary["source"]
    require(
        source["head_tree_sha"] == source["checkout_tree_sha"],
        "Head/checkout trees differ",
    )
    merge = json.loads((ROOT / "merge-summary.json").read_text())
    parity = json.loads((ROOT / "local-source-parity.json").read_text())
    require(merge["merge_sha"] == source["checkout_sha"], "Merge commit differs")
    require(
        merge["merge_tree_sha"] == source["checkout_tree_sha"], "Merge tree differs"
    )
    require(
        parity["head_sha"] == merge["head_sha"] == source["head_sha"],
        "Head commit differs",
    )
    require(parity["head_tree_sha"] == source["head_tree_sha"], "Head tree differs")
    require(
        parity["matches_successful_local_run"]
        and parity["source_files"] == source["local_source_files"] == 630,
        "Local source parity",
    )
    require(
        parity["local_source_inventory_sha256"]
        == source["local_source_inventory_sha256"],
        "Local source inventory digest",
    )
    require(len(summary["runs"]) == 7, "Workflow count")
    for run in summary["runs"]:
        require(
            run["head_sha"] == source["head_sha"]
            and run["conclusion"] == "success"
            and run["event"] == "pull_request",
            "Workflow identity/status",
        )
    jobs = {job["id"]: job for job in summary["jobs"]}
    require(len(jobs) == 16, "Job count")
    require(
        sum(j["conclusion"] == "success" for j in jobs.values()) == 15,
        "Successful job count",
    )
    require(
        sum(j["conclusion"] == "skipped" for j in jobs.values()) == 1,
        "Skipped job count",
    )
    logs = {}
    for job in jobs.values():
        require(job["head_sha"] == source["head_sha"], "Job head differs")
        require(
            all("macos" not in label.lower() for label in job["labels"]),
            "Unexpected macOS job",
        )
        if job["conclusion"] == "skipped":
            require(
                job["name"] == "campaign" and "log" not in job, "Unexpected skipped job"
            )
            continue
        text = compressed(job["log"]).decode()
        logs[job["id"]] = text.splitlines()
        checkout = re.findall(r"log -1 --format=%H\r?\n[^\n]*?\b([0-9a-f]{40})\b", text)
        require(
            checkout == [source["checkout_sha"]]
            and job["checkout_sha"] == source["checkout_sha"],
            "Checkout commit differs",
        )
        require(
            job["checkout_tree_sha"] == source["checkout_tree_sha"],
            "Checkout tree differs",
        )
        for observation in job["rust_version_observations"]:
            require(
                logs[job["id"]][observation["line_number"] - 1] == observation["text"],
                "Rust version line differs",
            )
            require(
                observation["version"] in observation["text"], "Rust version differs"
            )
    pattern = re.compile(
        r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;"
    )
    for record in summary["selected_step_results"]:
        lines = logs[record["job_id"]]
        start, end = record["line_start"] - 1, record["line_end_exclusive"] - 1
        require(
            lines[start] == record["command_group"] and "##[group]" in lines[start],
            "Command boundary differs",
        )
        require(
            not any("##[group]" in line for line in lines[start + 1 : end]),
            "Command boundary crosses next group",
        )
        require(
            (lines[end] if end < len(lines) else None) == record["next_group"],
            "Next boundary differs",
        )
        expected = []
        for i in range(start, end):
            match = pattern.search(lines[i])
            if match:
                expected.append(
                    {
                        "line_number": i + 1,
                        "text": lines[i],
                        "status": match[1],
                        "passed": int(match[2]),
                        "failed": int(match[3]),
                        "ignored": int(match[4]),
                    }
                )
        require(
            json.loads(compressed(record["result_stream"])) == expected,
            "Result stream differs",
        )
        counts = {
            key: sum(row[key] for row in expected)
            for key in ("passed", "failed", "ignored")
        }
        counts["groups"] = len(expected)
        require(
            counts == record["counts"] and counts["failed"] == 0, "Test counts differ"
        )
        step = next(
            s
            for s in jobs[record["job_id"]]["steps"]
            if s["number"] == record["step_number"]
        )
        require(
            step["name"] == record["step_name"]
            and step["conclusion"] == record["step_conclusion"] == "success",
            "Step status differs",
        )
    for record in summary["api_records"]:
        json.loads(compressed(record))
    print(
        json.dumps(
            {
                "status": "passed",
                "files": len(manifest["files"]),
                "workflow_runs": 7,
                "successful_jobs": 15,
                "skipped_jobs": 1,
                "exact_command_result_streams": len(summary["selected_step_results"]),
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
