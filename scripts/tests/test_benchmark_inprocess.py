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
    def run_case(self, changed_output=False, changed_header=False):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            header = root / "api.h"
            header.write_text("int f(void);\n")
            binary = root / "driver"
            binary.write_bytes(b"driver")
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
                    for engine in ("toucan", "bindgen")
                },
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
                    json.dumps({"engine": engine, "samples_ms": [1.0] * 3}).encode(),
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
            ]
            with (
                patch.object(sys, "argv", arguments),
                patch.object(benchmark, "execute", side_effect=execute) as run,
                patch.object(benchmark, "cpu_model", return_value="test"),
            ):
                if changed_output or changed_header:
                    with self.assertRaisesRegex(ValueError, "changed from reference"):
                        benchmark.main()
                else:
                    benchmark.main()
                report = json.loads((output / "evidence.json").read_text())
                self.assertEqual(
                    report["status"],
                    "failed" if changed_output or changed_header else "passed",
                )
                self.assertEqual(
                    run.call_count, 0 if changed_header else 1 if changed_output else 6
                )

    def test_complete_outputs_match_the_reference(self):
        self.run_case()

    def test_changed_output_fails_and_preserves_evidence(self):
        self.run_case(changed_output=True)

    def test_changed_header_fails_before_measurement(self):
        self.run_case(changed_header=True)
