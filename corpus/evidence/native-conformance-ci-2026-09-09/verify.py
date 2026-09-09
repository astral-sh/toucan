#!/usr/bin/env python3
"""Verify the preserved stable-CI reports and command protocols without compilers."""

import gzip
import hashlib
import json
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
        raw = stream.read(128 * 1024 * 1024 + 1)
    require(len(raw) <= 128 * 1024 * 1024, f"oversized payload: {name}")
    return json.loads(raw)


def clean(command):
    return (
        not command["timeout"]
        and "start_error" not in command
        and not command.get("crash_diagnostic", False)
    )


def accepted(command):
    return command["exit_code"] == 0 and clean(command)


def commands(value):
    if isinstance(value, dict):
        if "command" in value and "exit_code" in value:
            yield value
        for nested in value.values():
            yield from commands(nested)
    elif isinstance(value, list):
        for nested in value:
            yield from commands(nested)


def main():
    manifest = load("summary.json")
    for name, expected in manifest["files"].items():
        data = (ROOT / name).read_bytes()
        require(len(data) == expected["bytes"], f"payload size: {name}")
        require(digest(data) == expected["sha256"], f"payload hash: {name}")
    metadata = load("metadata.json")
    sources = load("tested-sources.json")
    build = load("build-verification.json")
    merge = load("tested-merge.json")
    artifacts = load("artifact-verification.json")
    capture = compressed("capture.json.gz")
    require(metadata["run"]["conclusion"] == "success", "CI run failed")
    require(metadata["run"]["head_sha"] == sources["head_sha"], "head revision")
    require(
        build["tested_tree"] == sources["tree"] == merge["tree"]["sha"],
        "tested merge differs from head tree",
    )
    require(merge["sha"] == build["actual_tested_commit"], "tested merge commit")
    require(
        digest((ROOT / "tested-merge.json").read_bytes())
        == build["merge_metadata_sha256"],
        "merge metadata changed",
    )
    for key, text in capture["contents"].items():
        require(digest(text.encode()) == key, "captured content hash")
    for name, entry in capture["files"].items():
        require(
            len(capture["contents"][entry["sha256"]].encode()) == entry["bytes"],
            f"captured content size: {name}",
        )
        require(
            "/source/" not in name
            and not name.endswith(".i")
            and not name.endswith("native-declarations.txt"),
            f"excluded upstream payload: {name}",
        )

    def captured(name):
        return capture["contents"][capture["files"][name]["sha256"]]

    for path, entry in sources["files"].items():
        raw = captured(f"tested-git/{path}").encode()
        require(digest(raw) == entry["sha256"], f"tested source: {path}")
        git_blob = hashlib.sha1(
            f"blob {len(raw)}\0".encode() + raw, usedforsecurity=False
        ).hexdigest()
        require(git_blob == entry["git_blob"], f"Git blob: {path}")
        require(
            entry["sha256"] == build["head_file_sha256"][path],
            f"independent source review: {path}",
        )
    for name, expected in build["log_file_sha256"].items():
        require(digest(captured(f"ci-logs/{name}").encode()) == expected, name)

    counts = {
        "commands": 0,
        "streams": 0,
        "native_requests": 0,
        "native_configuration_controls": 0,
    }
    profiles = {}
    for profile in ["gcc", "clang"]:
        report = compressed(f"{profile}.json.gz")
        config = report["configuration"]
        native_config = report["native_source_configuration"]
        require(report["schema_version"] == 2, f"{profile}: report schema")
        require(config["native_source"], "native route disabled")
        require(config["fail_on_strict_difference"], "strict gate disabled")
        require(config["compiler"] == profile, "wrong compiler profile")
        require(len(report["cases"]) == 220, "incomplete source selection")
        require(
            report["runner_sha256"]
            == sources["files"]["scripts/audit_c_testsuite.py"]["sha256"],
            "tested driver hash mismatch",
        )
        require(
            report["tools"]["native_probe"]["protocol_source"]["sha256"]
            == sources["files"]["crates/toucan/examples/audit_translation_unit.rs"][
                "sha256"
            ],
            "tested probe source hash mismatch",
        )
        require(
            report["manifest_sha256"]
            == sources["files"]["corpus/conformance/c-testsuite.json"]["sha256"],
            "tested source manifest hash mismatch",
        )
        require(
            report["manifest"]["archive_sha256"] == artifacts["archive"]["sha256"],
            "independently verified source archive mismatch",
        )
        for key in ["toucan", "native_probe"]:
            field = (
                "recorded_cli_sha256" if key == "toucan" else "recorded_probe_sha256"
            )
            require(
                report["tools"][key]["sha256"] == build["profiles"][profile][field],
                f"{profile}: recorded binary identity",
            )
        artifact = next(
            a
            for a in metadata["artifacts"]["artifacts"]
            if a["name"] == f"c-testsuite-conformance-{profile}"
        )
        require(
            artifact["digest"]
            == f"sha256:{artifacts['profiles'][profile]['artifact_sha256']}",
            "downloaded artifact digest mismatch",
        )
        require(
            not report["changed_native_dependencies"]
            and not report["summary"]["changed_tools"]
            and not report["summary"]["tool_failures"]
            and not report["summary"]["oracle_pipeline_failures"],
            f"{profile}: infrastructure or input instability",
        )

        def relative(path, profile=profile):
            return f"{profile}/" + path.split("/corpus/results/conformance/", 1)[1]

        control = native_config["probe_control"]
        control_result = native_config["probe_configuration"]
        require(
            accepted(control) and control_result["status"] == "preprocessed",
            "native configuration control status/exit mismatch",
        )
        require(
            json.loads(captured(relative(control["stdout"]))) == control_result,
            "native configuration control JSON mismatch",
        )
        control_request = captured(relative(control["command"][1])).encode()
        require(
            digest(control_request) == control["request_sha256"], "control request hash"
        )
        control_options = json.loads(control_request)
        require(control_options["operation"] == "preprocess", "control operation")
        for key in [
            "target",
            "compiler",
            "language_mode",
            "include_dirs",
            "definitions",
        ]:
            require(control_options[key] == native_config[key], "control configuration")
        counts["native_configuration_controls"] += 1

        for command in commands(report):
            counts["commands"] += 1
            require(clean(command), "command crashed, timed out, or failed to start")
            require(
                command["environment_overrides"]
                == {"LC_ALL": "C", "SOURCE_DATE_EPOCH": "0"},
                "unstable subprocess clock or locale",
            )
            for stream in ["stdout", "stderr"]:
                captured(relative(command[stream]))
                counts["streams"] += 1
            if "output" in command:
                output = command["output"]
                entry = capture["excluded_artifacts"][relative(output["path"])]
                require(entry["sha256"] == output["sha256"], "recorded output hash")
                require(entry["bytes"] == output["bytes"], "recorded output size")

        eligible = []
        strict = []
        legacy_accepted = []
        native_accepted = []
        all_dependencies = {}
        for case in report["cases"]:
            name = case["name"]
            source_hash = capture["excluded_artifacts"][relative(case["source"])][
                "sha256"
            ]
            require(source_hash == case["source_sha256"], f"{name}: source hash")
            require(
                source_hash == artifacts["profiles"][profile]["source_hashes"][name],
                f"{name}: pinned archive source comparison",
            )
            require(accepted(case["preprocess"]), f"{name}: oracle preprocess")
            require(
                json.loads(captured(f"{profile}/cases/{Path(name).stem}/result.json"))
                == case,
                f"{name}: inconsistent per-case sidecar",
            )
            both = {
                dialect: all(
                    accepted(case["compilers"][compiler][dialect])
                    for compiler in ["gcc", "clang"]
                )
                for dialect in case["both_accept"]
            }
            require(both == case["both_accept"], f"{name}: oracle acceptance")
            is_eligible = both[config["dialect"]] and accepted(
                case["preprocessed_oracle"]
            )
            require(is_eligible == case["eligible"], f"{name}: eligibility")
            if is_eligible:
                eligible.append(name)
                if both["strict_c11"]:
                    strict.append(name)
            if accepted(case["toucan"]):
                legacy_accepted.append(name)
            require(
                case["difference"] == (is_eligible and not accepted(case["toucan"])),
                f"{name}: legacy difference classification",
            )
            native = case["native_source"]
            require(
                native["source_before"] == native["source_after"] == source_hash,
                f"{name}: source changed during native execution",
            )
            require(
                native["dependencies_before"] == native["dependencies_after"]
                and not native["changed_dependencies"],
                f"{name}: native inputs changed",
            )
            dependencies = native["dependencies_before"]
            require(
                set(native["preprocessing_result"]["dependencies"])
                == set(dependencies),
                f"{name}: actual preprocessing dependency set",
            )
            for path, value in dependencies.items():
                require(
                    report["native_dependency_hashes"][path] == value,
                    "final input hash",
                )
                if path in all_dependencies:
                    require(all_dependencies[path] == value, "cross-case input hash")
                all_dependencies[path] = value
            require(
                native["preprocessing_result"]["status"] == "preprocessed"
                and accepted(native["preprocess"]),
                f"{name}: native preprocessing status/exit mismatch",
            )
            for key in [
                "compiler",
                "definitions",
                "embedded_headers",
                "feature_queries",
            ]:
                require(
                    native["preprocessing_result"][key] == control_result[key],
                    f"{name}: shipped configuration differs from control",
                )
            result = native["analysis_result"]
            if native["status"] == "accepted":
                require(
                    result["status"] == "accepted" and accepted(native["analysis"]),
                    name,
                )
                require(
                    set(result["dependencies"]) == set(dependencies), "analysis inputs"
                )
                native_accepted.append(name)
            else:
                require(
                    native["status"] == "analysis_rejected"
                    and result["status"] == "rejected"
                    and result["stage"] == "analysis"
                    and native["analysis"]["exit_code"] == 1
                    and clean(native["analysis"]),
                    f"{name}: native rejection status/exit mismatch",
                )
            require(
                native["difference"]
                == (is_eligible and native["status"] != "accepted"),
                f"{name}: native difference classification",
            )
            for operation, key in [
                ("preprocess", "preprocessing_result"),
                ("analyze", "analysis_result"),
            ]:
                command = native[
                    "preprocess" if operation == "preprocess" else "analysis"
                ]
                raw_request = captured(relative(command["command"][1])).encode()
                require(
                    digest(raw_request) == command["request_sha256"], "request hash"
                )
                request = json.loads(raw_request)
                require(request["operation"] == operation, "probe operation")
                require(request["input"] == case["source"], "probe original source")
                for config_key in [
                    "target",
                    "compiler",
                    "language_mode",
                    "include_dirs",
                    "definitions",
                ]:
                    require(
                        request[config_key] == native_config[config_key],
                        "probe configuration",
                    )
                require(request["retain_code"] is False, "unexpected retained code")
                require(
                    json.loads(captured(relative(command["stdout"]))) == native[key],
                    f"{name}: raw native JSON result",
                )
                counts["native_requests"] += 1
        require(
            all_dependencies == report["native_dependency_hashes"],
            "final dependency set",
        )
        require(len(eligible) == 220 and len(strict) == 211, "eligible case counts")
        require(set(strict) <= set(legacy_accepted), "legacy strict acceptance")
        require(set(strict) <= set(native_accepted), "native strict acceptance")
        require(
            set(eligible) - set(legacy_accepted) == {"00144.c"}, "legacy difference"
        )
        require(
            set(eligible) - set(native_accepted) == {"00144.c"}, "native difference"
        )
        require(
            len(legacy_accepted) == len(native_accepted) == 219, "exploratory counts"
        )
        require(
            report["summary"] == manifest["profiles"][profile]["summary"], "summary"
        )
        require(
            report["summary"]["strict_toucan_accepted"] == len(strict)
            and report["summary"]["native_source"]["strict_accepted"] == len(strict),
            "recorded strict acceptance counts",
        )
        profiles[profile] = {
            "sources": len(eligible),
            "strict_positive": len(strict),
            "legacy_accepted": len(legacy_accepted),
            "native_accepted": len(native_accepted),
            "native_files": len(all_dependencies),
            "native_headers": len(all_dependencies) - len(eligible),
        }
    print(
        json.dumps(
            {"status": "pass", "profiles": profiles, "verified": counts}, indent=2
        )
    )


if __name__ == "__main__":
    main()
