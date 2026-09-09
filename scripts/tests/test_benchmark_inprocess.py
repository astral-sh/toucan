import hashlib
import json
import statistics
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
        builder_roots="allowlist_files",
        empty_roots=False,
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
                    builder_roots: [] if empty_roots else [str(header)],
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
                            "warmup_ms": 2.0,
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
                    or empty_roots
                )
                if failed:
                    with self.assertRaisesRegex(
                        ValueError,
                        "captured configurations"
                        if missing_configuration or empty_roots
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
                    if changed_header or missing_configuration or empty_roots
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

    def test_builder_name_roots_use_captured_policy_and_outputs(self):
        for category in ("allowlist_types", "allowlist_functions", "allowlist_vars"):
            with self.subTest(category=category):
                self.run_case(builder=True, builder_roots=category)

    def test_builder_empty_selection_fails_before_measurement(self):
        self.run_case(builder=True, empty_roots=True)

    def test_paired_summary_uses_process_medians_and_matching_pairs(self):
        # Pooling calls yields 3 and 4; process medians yield 2 and 4. The
        # median paired ratio is 3, distinct from either ratio of medians.
        samples = {
            "toucan-builder": [[1, 1, 99], [2, 2, 99], [3, 4, 5]],
            "bindgen": [[3, 3, 300], [8, 8, 8], [4, 4, 4]],
        }
        rows = [
            {
                "pair": pair,
                "engine": engine,
                "samples_ms": values,
                "warmup_ms": 10 * statistics.median(values),
            }
            for engine, processes in samples.items()
            for pair, values in enumerate(processes)
        ]
        result = benchmark.paired_summary(rows[::-1], tuple(samples), 3)
        self.assertEqual(result["median_ms"], {"toucan-builder": 2, "bindgen": 4})
        self.assertEqual(result["bindgen_over_toucan_ratios"], [3, 4, 1])
        self.assertEqual(result["median_bindgen_over_toucan"], 3)
        self.assertEqual(result["bindgen_over_toucan_range"], [1, 4])
        self.assertEqual(
            result["median_first_call_ms"], {"toucan-builder": 20, "bindgen": 40}
        )
        with self.assertRaisesRegex(ValueError, "duplicate"):
            benchmark.paired_summary([*rows, rows[0]], tuple(samples), 3)
        with self.assertRaisesRegex(ValueError, "incomplete"):
            benchmark.paired_summary(rows[1:], tuple(samples), 3)
