import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import benchmark_inprocess as benchmark


class InprocessBenchmarkTests(unittest.TestCase):
    def run_case(
        self,
        changed_output=False,
        changed_header=False,
        builder=False,
        changed_configuration=False,
        missing_configuration=False,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            header = root / "api.h"
            header.write_text("int f(void);\n")
            binary = root / "driver"
            binary.write_bytes(b"driver")
            toucan_engine = "toucan-builder" if builder else "toucan"
            reference = {
                "commands": {
                    "toucan": [
                        "toucan",
                        "bindgen",
                        str(header),
                        "--target",
                        "x86_64-unknown-linux-gnu",
                        "--sysroot",
                        "/",
                    ],
                    "bindgen": ["bindgen", str(header)],
                },
                "dependency_sha256": {str(header): benchmark.digest(header)},
                "observations": {
                    engine: [
                        {"output_sha256": hashlib.sha256(engine.encode()).hexdigest()}
                    ]
                    for engine in (toucan_engine, "bindgen")
                },
            }
            if builder:
                reference["request"] = {
                    **benchmark.request_from_reference(reference),
                    "policy": "builder",
                    "generate_comments": True,
                    "allowlist_files": [str(header)],
                }
                if not missing_configuration:
                    reference["configurations"] = {
                        engine: reference["request"]
                        for engine in (toucan_engine, "bindgen")
                    }
            path = root / "api.json"
            path.write_text(json.dumps(reference))
            output = root / "results"
            if changed_header:
                header.write_text("long f(void);\n")

            def execute(command, timeout):
                engine = command[1]
                Path(command[-1]).write_text("changed" if changed_output else engine)
                return subprocess.CompletedProcess(
                    command,
                    0,
                    json.dumps(
                        {
                            "engine": engine,
                            "samples_ms": [1.0] * 3,
                            "configuration": {}
                            if changed_configuration
                            else reference.get("request"),
                        }
                    ).encode(),
                    b"",
                )

            arguments = [
                "benchmark",
                str(path),
                "--binary",
                str(binary),
                "--output",
                str(output),
                "--iterations",
                "3",
                "--toucan-engine",
                toucan_engine,
            ]
            with (
                patch.object(sys, "argv", arguments),
                patch.object(benchmark, "execute", side_effect=execute) as run,
                patch.object(benchmark, "cpu_model", return_value="test"),
            ):
                failed = (
                    changed_output
                    or changed_header
                    or changed_configuration
                    or missing_configuration
                )
                if failed:
                    with self.assertRaisesRegex(
                        ValueError,
                        "captured configurations"
                        if missing_configuration
                        else "changed from reference",
                    ):
                        benchmark.main()
                else:
                    benchmark.main()
                report = json.loads((output / "evidence.json").read_text())
                self.assertEqual(
                    report["status"],
                    "failed" if failed else "passed",
                )
                self.assertEqual(
                    run.call_count,
                    0
                    if changed_header or missing_configuration
                    else 1
                    if changed_output or changed_configuration
                    else 6,
                )

    def test_complete_outputs_match_the_reference(self):
        self.run_case()

    def test_changed_output_fails_and_preserves_evidence(self):
        self.run_case(changed_output=True)

    def test_changed_header_fails_before_measurement(self):
        self.run_case(changed_header=True)

    def test_builder_route_uses_captured_policy_and_outputs(self):
        self.run_case(builder=True)

    def test_builder_configuration_changes_fail(self):
        self.run_case(builder=True, changed_configuration=True)

    def test_builder_configuration_is_required_before_measurement(self):
        self.run_case(builder=True, missing_configuration=True)
