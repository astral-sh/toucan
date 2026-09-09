#!/usr/bin/env python3
"""Verify the local matrix evidence without executing compilers or fetching sources."""

import gzip
import hashlib
import json
import re
import runpy
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load(name):
    return json.loads((ROOT / name).read_bytes())


def compressed(name):
    with gzip.open(ROOT / name, "rb") as stream:
        raw = stream.read(32 * 1024 * 1024 + 1)
    require(len(raw) <= 32 * 1024 * 1024, f"oversized payload: {name}")
    return json.loads(raw)


def totals(log):
    outcomes = re.findall(
        r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", log
    )
    return {
        "passed": sum(int(row[1]) for row in outcomes),
        "failed": sum(int(row[2]) for row in outcomes),
        "ignored": sum(int(row[3]) for row in outcomes),
        "groups": len(outcomes),
    }


def main():
    manifest = load("manifest.json")
    actual = {
        path.relative_to(ROOT).as_posix() for path in ROOT.rglob("*") if path.is_file()
    }
    require(
        actual == set(manifest["files"]) | {"manifest.json"}, "package file inventory"
    )
    require(not any(path.is_symlink() for path in ROOT.rglob("*")), "package symlink")
    require(
        not any(Path(path).name == "Cargo.toml" for path in actual),
        "active Cargo manifest",
    )
    for name, entry in manifest["files"].items():
        raw = (ROOT / name).read_bytes()
        require(len(raw) == entry["bytes"] and digest(raw) == entry["sha256"], name)
    capture = compressed("local-capture.json.gz")
    independent = compressed("local-independent-review.json.gz")
    require(
        independent["status"] == "passed" and not independent["findings"],
        "independent local review",
    )
    require(
        digest((ROOT / "local-capture.json.gz").read_bytes())
        == independent["local_capture_sha256"],
        "independently reviewed capture changed",
    )
    for key, text in capture["contents"].items():
        require(digest(text.encode()) == key, "captured text hash")
    for path, entry in capture["files"].items():
        require(
            len(capture["contents"][entry["sha256"]].encode()) == entry["bytes"], path
        )
        require(
            independent["original_files_sha256"][path] == entry["sha256"],
            "original source review mismatch",
        )

    def raw(path):
        return capture["contents"][capture["files"][path]["sha256"]].encode()

    def document(path):
        return json.loads(raw(path))

    def match_file(path, expected):
        require(digest(raw(path)) == expected, f"recorded hash: {path}")

    local = document("/tmp/toucan-armv7-test-matrix-manifest.json")

    def recorded_files(value):
        if isinstance(value, dict):
            if "path" in value and "sha256" in value:
                match_file(value["path"], value["sha256"])
            for child in value.values():
                recorded_files(child)
        elif isinstance(value, list):
            for child in value:
                recorded_files(child)

    recorded_files(local)
    review = document(local["independent_review"]["path"])
    require(review["status"] == "passed" and not review["findings"], "combined review")
    before = document(local["source_before"]["path"])
    after = document(local["source_after"]["path"])
    require(before["source"] == after["source"], "source changed during workspace test")
    require(len(before["source"]) == 630, "source inventory size")
    identity = load("git-source-verification.json")
    parity = load("local-source-parity.json")
    merge = load("merge-summary.json")
    require(identity["status"] == "passed", "Git source verification")
    require(
        identity["head"]
        == parity["head_sha"]
        == merge["head_sha"]
        == "6c66ec7266165c1be28e372edb2bfdccee252a01",
        "Git source identity",
    )
    require(
        identity["source_inventory_sha256"]
        == parity["local_source_inventory_sha256"]
        == local["source_after"]["sha256"],
        "Git/local inventory mismatch",
    )
    require(
        identity["source_file_count"] == parity["source_files"] == 630
        and parity["matches_successful_local_run"],
        "Git/local file count",
    )
    require(merge["merge_tree_sha"] == parity["head_tree_sha"], "merge tree")
    for key in [
        "source_before",
        "source_after",
        "independent_review",
        "workspace_summary",
    ]:
        match_file(local[key]["path"], local[key]["sha256"])
    require(len(local["files"]) == 20, "changed-file count")
    for path, entry in local["files"].items():
        match_file(f"before/{path}", entry["before_sha256"])
        match_file(f"after/{path}", entry["after_sha256"])
        require(review["changed_files_sha256"][path] == entry["after_sha256"], path)
        if path != "README.md":
            require(before["source"][path] == entry["after_sha256"], path)
        if "/tests/" not in path and path != "README.md":
            require(path == "crates/toucan_semantic/src/checked/statement.rs", path)
            old, new = raw(f"before/{path}"), raw(f"after/{path}")
            require(b"#[cfg(test)]" in old and b"#[cfg(test)]" in new, "test boundary")
            require(
                old.split(b"#[cfg(test)]", 1)[0] == new.split(b"#[cfg(test)]", 1)[0],
                "production statement implementation changed",
            )
    require(
        before["readme_sha256"] == local["files"]["README.md"]["after_sha256"],
        "README identity",
    )
    match_file(local["patch"]["path"], local["patch"]["sha256"])
    workspace = local["workspace"]
    require(workspace["exit_code"] == 0, "workspace command failed")
    match_file(workspace["log"]["path"], workspace["log"]["sha256"])
    counts = totals(raw(workspace["log"]["path"]).decode())
    require(
        counts == {"passed": 1093, "failed": 0, "ignored": 263, "groups": 296}, counts
    )
    for field in ["passed", "failed", "ignored"]:
        require(workspace[field] == counts[field], "workspace reported counts")
    require(workspace["test_binaries_and_doctest_groups"] == counts["groups"], "groups")
    oracles = document(local["targeted_ignored_oracles"]["path"])
    require(len(oracles) == 3, "targeted ignored test count")
    for result in oracles:
        require(result["exit_code"] == 0 and "--ignored" in result["command"], result)
        match_file(result["log"], result["log_sha256"])
        text = raw(result["log"]).decode()
        require(
            totals(text) == {"passed": 1, "failed": 0, "ignored": 0, "groups": 1},
            result,
        )
        require(f"test {result['test']} ... ok" in text, result["test"])

    probe_count = 0
    for directory, filename, key in [
        ("toucan-armv7-target-expectations-oracle", "evidence.json", "commands"),
        (
            "toucan-armv7-target-expectations-oracle",
            "supplemental-evidence.json",
            "commands",
        ),
        ("toucan-armv7-semantic-expectations-oracle", "evidence.json", "cases"),
    ]:
        prefix = f"/tmp/{directory}"
        report = document(f"{prefix}/{filename}")
        require(report["status"] == "passed", filename)
        for command in report[key]:
            name = command["name"]
            argv = command.get("argv", command.get("command"))
            source = command.get("source", f"{prefix}/{name}.c")
            require(source in argv, "probe source differs from command")
            expected_exit = command.get(
                "expected_exit_code", command.get("expected_exit")
            )
            require(command["exit_code"] == expected_exit, name)
            match_file(source, command["source_sha256"])
            for stream in ["stdout", "stderr"]:
                match_file(f"{prefix}/{name}.{stream}", command[f"{stream}_sha256"])
            diagnostic = command.get("diagnostic_contains")
            if diagnostic:
                require(diagnostic in raw(f"{prefix}/{name}.stderr").decode(), name)
            if "-emit-llvm" in argv:
                raw(argv[argv.index("-o") + 1])
            probe_count += 1
    overflow = document("/tmp/toucan-overflow-armv7-oracle/verification.json")
    source = "/tmp/toucan-overflow-armv7-oracle/int128.c"
    require(raw(source).decode() == overflow["source"], "overflow original source")
    require(len(overflow["results"]) == 4, "overflow target count")
    for result in overflow["results"]:
        unsupported = result["target"] in [
            "armv7-unknown-linux-gnueabihf",
            "i686-unknown-linux-gnu",
        ]
        require(source in result["command"], "overflow source command")
        require(result["exit_code"] == int(unsupported), result["target"])
        if unsupported:
            require(
                "__int128 is not supported on this target" in result["stderr"], result
            )
        probe_count += 1
    require(probe_count == 30, "direct compiler probe count")

    historical = load("historical-armv7-layout/manifest.json")
    for name, entry in historical["files"].items():
        data = (ROOT / "historical-armv7-layout" / name).read_bytes()
        require(digest(data) == entry["sha256"] and len(data) == entry["bytes"], name)
    evidence = compressed("historical-armv7-layout/evidence.json.gz")
    inputs = compressed("historical-armv7-layout/verification-inputs.json.gz")
    layout_review = document(
        "/tmp/toucan-armv7-target-expectations-oracle/layout-evidence-review.json"
    )
    require(
        historical["source_commit"]
        == layout_review["source_commit"]
        == "8c8e74c7aa5598da20725b9247836a3559189d46",
        "historical source identity",
    )
    require(
        evidence["status"] == layout_review["status"] == "passed"
        and evidence["qemu_execution"]
        and not evidence["native_hardware_execution"],
        "historical evidence scope",
    )
    require(len(evidence["commands"]) == 52, "historical command count")
    for command in evidence["commands"]:
        require(command["exit_code"] == 0, command["name"])
        require(
            layout_review["command_status_and_log_hashes"][command["name"]], command
        )
        for stream in ["stdout", "stderr"]:
            require(
                digest(inputs[f"{command['name']}.{stream}"].encode())
                == command[f"{stream}_sha256"],
                command["name"],
            )
    for name, expected in evidence["source_sha256"].items():
        require(digest(inputs[name].encode()) == expected, name)
        require(layout_review["source_checks"][name], name)
    require(
        inputs["layout.c"] == layout_review["retained_c_layout_source"], "layout source"
    )
    require(inputs["layout.c"].count("_Static_assert(") == 16, "layout assertions")
    require(
        [row for row in evidence["commands"] if row["name"] == "clang-layout-object"]
        == layout_review["clang_layout_command"],
        "historical layout command",
    )
    binding = json.loads(inputs["binding-report.json"])
    require(binding["compiler"] == layout_review["binding_report_compiler"], "profile")
    require(binding["target"] == layout_review["binding_report_target"], "target")

    original = load("original-ci/summary.json")
    for path, entry in original["files"].items():
        data = (ROOT / "original-ci" / path).read_bytes()
        require(digest(data) == entry["sha256"] and len(data) == entry["bytes"], path)
    ci_logs = compressed("original-ci/logs.json.gz")
    for name, entry in original["log_inventory"].items():
        data = ci_logs[name].encode()
        require(digest(data) == entry["sha256"] and len(data) == entry["bytes"], name)
    require(
        original["head"] == "39a4c0c0992ca943745268452a31a5d2f6079313", "old CI head"
    )
    require(
        original["workflows"]["CI"]["conclusion"] == "failure", "old CI failure hidden"
    )
    failures = compressed("original-ci/failures.json.gz")
    for architecture in ["arm", "x64"]:
        require(len(failures[architecture]["panics"]) == 21, "original failure count")
        require(
            len(failures[architecture]["failed_cargo_targets"]) == 19, "target count"
        )
        tests = re.findall(
            r"thread '(.+)' \([0-9]+\) panicked at",
            ci_logs[f"full-tests-{architecture}.log"],
        )
        require(
            set(tests) == {f["test"] for f in failures[architecture]["panics"]},
            "failure log mismatch",
        )

    # Keep the captured CI record and its verifier independently usable.
    runpy.run_path(str(ROOT / "stable-ci" / "verify.py"), run_name="__main__")
    stable = load("stable-ci/summary.json")
    source = stable["source"]
    require(source["head_sha"] == identity["head"], "CI/local Git head")
    require(source["checkout_sha"] == merge["merge_sha"], "CI merge identity")
    require(source["checkout_tree_sha"] == parity["head_tree_sha"], "CI tree identity")
    require(
        source["local_source_inventory_sha256"] == local["source_after"]["sha256"]
        and source["local_source_files"] == 630,
        "CI/local source inventory",
    )
    require(
        stable["counts"]
        == {
            "workflow_runs": 7,
            "executed_jobs": 15,
            "successful_jobs": 15,
            "skipped_jobs": 1,
        },
        "CI outcome counts",
    )
    expected = {
        (102631425748, "generated_rust_1_64"): (10, 0, 2),
        (102631425987, "generated_rust_1_64"): (10, 0, 2),
        (102631425118, "workspace_all_features"): (1086, 251, 296),
        (102631425118, "workspace_package"): (0, 0, 0),
        (102631425192, "parameter_entry_oracles"): (9, 0, 1),
        (102631425192, "workspace_include_ignored"): (1363, 0, 297),
        (102631425206, "workspace_all_features"): (1093, 263, 296),
        (102631425206, "workspace_package"): (0, 0, 0),
        (102631425220, "parameter_entry_oracles"): (9, 0, 1),
        (102631425220, "workspace_include_ignored"): (1360, 0, 297),
    }
    commands = stable["selected_step_results"]
    require(len(commands) == len(expected), "CI command result count")
    require(
        {(row["job_id"], row["category"]) for row in commands} == set(expected),
        "CI command identity",
    )
    for row in commands:
        passed, ignored, groups = expected[row["job_id"], row["category"]]
        require(
            row["counts"]
            == {
                "passed": passed,
                "failed": 0,
                "ignored": ignored,
                "groups": groups,
            },
            "CI command totals",
        )
    print(
        "Verified original CI failures, 30 compiler probes, 1,093 local passes "
        "with 263 ignored, three selected ignored passes, 630 stable source hashes, "
        "and seven successful CI workflows with 15 successful jobs."
    )


if __name__ == "__main__":
    main()
