#!/usr/bin/env python3
"""Verify the captured native-source audit without compilers or downloaded inputs."""

import gzip
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def digest(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def load_gzip(name):
    with gzip.open(ROOT / name, "rb") as stream:
        data = stream.read(128 * 1024 * 1024 + 1)
    require(len(data) <= 128 * 1024 * 1024, f"oversized capture: {name}")
    return json.loads(data)


def accepted(command):
    return (
        command["exit_code"] == 0
        and not command["timeout"]
        and "start_error" not in command
        and not command.get("crash_diagnostic", False)
    )


def main():
    summary = json.loads((ROOT / "summary.json").read_text())
    for name, expected in summary["files"].items():
        data = (ROOT / name).read_bytes()
        require(len(data) == expected["bytes"], f"size mismatch: {name}")
        require(digest(data) == expected["sha256"], f"hash mismatch: {name}")
    capture = load_gzip("capture.json.gz")
    for key, text in capture["contents"].items():
        require(digest(text.encode()) == key, f"content hash mismatch: {key}")
    for name, entry in capture["files"].items():
        data = capture["contents"][entry["sha256"]].encode()
        require(len(data) == entry["bytes"], f"captured size mismatch: {name}")
        require(
            "/source/" not in name
            and not name.endswith(".i")
            and not name.endswith("native-declarations.txt"),
            f"excluded upstream payload present: {name}",
        )
    driver = capture["files"]["candidate/scripts/audit_c_testsuite.py"]["sha256"]
    require(driver == summary["audit_driver"]["sha256"], "candidate driver hash")
    build = load_gzip("build.json.gz")
    require(build["exit_code"] == 0, "build failed")
    require(build["source_before"] == build["source_after"], "build source changed")
    require(len(build["source_before"]) == 630, "build source inventory")
    for profile in ["gcc", "clang"]:
        report = load_gzip(f"{profile}.json.gz")
        config = report["configuration"]
        require(report["runner_sha256"] == driver, f"{profile}: driver mismatch")
        require(config["native_source"] and config["compiler"] == profile, "profile")
        require(len(report["cases"]) == 220, f"{profile}: incomplete cases")
        require(report["summary"] == summary["profiles"][profile]["summary"], "summary")
        for name in ["tool_failures", "oracle_pipeline_failures", "changed_tools"]:
            require(not report["summary"][name], f"{profile}: {name}")
        require(not report["changed_native_dependencies"], "changed native inputs")
        require(
            report["tools"]["toucan"]["sha256"]
            == build["binaries"]["toucan"]["sha256"],
            "CLI build provenance",
        )
        require(
            report["tools"]["native_probe"]["sha256"]
            == build["binaries"]["examples/audit_translation_unit"]["sha256"],
            "probe build provenance",
        )
        strict = []
        for case in report["cases"]:
            eligible = all(
                accepted(case["compilers"][compiler][config["dialect"]])
                for compiler in ["gcc", "clang"]
            ) and accepted(case["preprocessed_oracle"])
            require(eligible == case["eligible"], f"{case['name']}: eligibility")
            if eligible and all(
                accepted(case["compilers"][compiler]["strict_c11"])
                for compiler in ["gcc", "clang"]
            ):
                strict.append(case)
            native = case["native_source"]
            require(
                native["status"] in ["accepted", "analysis_rejected"],
                f"{case['name']}: native pipeline failure",
            )
            require(
                native["preprocessing_result"]["status"] == "preprocessed"
                and accepted(native["preprocess"]),
                f"{case['name']}: native preprocessing status/exit mismatch",
            )
            analysis = native["analysis"]
            result = native["analysis_result"]
            if native["status"] == "accepted":
                require(
                    result["status"] == "accepted" and accepted(analysis),
                    f"{case['name']}: native acceptance status/exit mismatch",
                )
            else:
                require(
                    result["status"] == "rejected"
                    and result["stage"] == "analysis"
                    and analysis["exit_code"] == 1
                    and not analysis["timeout"]
                    and "start_error" not in analysis
                    and not analysis.get("crash_diagnostic", False),
                    f"{case['name']}: native rejection status/exit mismatch",
                )
            require(
                native["difference"] == (eligible and native["status"] != "accepted"),
                f"{case['name']}: native difference classification",
            )
            require(
                native["dependencies_before"] == native["dependencies_after"], "inputs"
            )
            stem = Path(case["name"]).stem
            sidecar_name = f"{profile}/cases/{stem}/result.json"
            sidecar = json.loads(
                capture["contents"][capture["files"][sidecar_name]["sha256"]]
            )
            require(sidecar == case, f"{case['name']}: stale sidecar")
            for operation, result_name in [
                ("preprocess", "preprocessing_result"),
                ("analyze", "analysis_result"),
            ]:
                name = f"{profile}/cases/{stem}/native-{operation}.stdout"
                recorded = json.loads(
                    capture["contents"][capture["files"][name]["sha256"]]
                )
                require(recorded == native[result_name], f"{name}: result mismatch")
        require(len(strict) == 211, f"{profile}: strict-positive membership")
        require(all(accepted(case["toucan"]) for case in strict), "legacy strict gate")
        require(
            all(case["native_source"]["status"] == "accepted" for case in strict),
            "native strict gate",
        )
        require(
            [case["name"] for case in report["cases"] if not accepted(case["toucan"])]
            == ["00144.c"],
            "legacy exploratory difference",
        )
        require(
            [
                case["name"]
                for case in report["cases"]
                if case["native_source"]["status"] != "accepted"
            ]
            == ["00144.c"],
            "native exploratory difference",
        )
    print(
        "Verified both 220-case profiles, 211 strict-positive cases per route, and capture hashes."
    )


if __name__ == "__main__":
    main()
