"""Protect conformance eligibility, bounded failures and complete-output parity."""

import json
import signal
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import audit_csmith as audit


class CsmithAuditTests(unittest.TestCase):
    def test_standalone_scripts_find_shared_diagnostics_with_safe_path(self):
        for name in (
            "audit_c_testsuite.py",
            "audit_csmith.py",
            "probe_inline_ownership.py",
        ):
            with self.subTest(script=name):
                result = subprocess.run(
                    [
                        sys.executable,
                        "-P",
                        str(Path(__file__).resolve().parents[1] / name),
                        "--help",
                    ],
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_crashes_and_timeouts_are_not_source_rejections(self):
        self.assertEqual(audit.classify(1, "constraint violation"), "rejected")
        self.assertEqual(
            audit.classify(1, "error: static assertion failed"), "rejected"
        )
        self.assertEqual(audit.classify(1, "internal compiler error"), "crash")
        self.assertEqual(audit.classify(-signal.SIGSEGV, ""), "crash")
        self.assertEqual(audit.classify(0, "", timeout=True), "timeout")
        self.assertEqual(audit.classify(-signal.SIGXFSZ, ""), "output_limit")
        self.assertEqual(
            audit.classify(1, "runtime error: overflow", kind="runtime"),
            "sanitizer_failure",
        )
        self.assertEqual(audit.classify(1, "", kind="generation"), "tool_error")

    @unittest.skipUnless(sys.platform == "linux", "process limits are Linux-only")
    def test_driver_failures_in_either_stream_cannot_count_as_rejection(self):
        diagnostics = [
            "clang: error: unable to execute command: Illegal instruction: 4",
            "LLVM ERROR: Cannot select instruction",
            "fatal error: error in backend: Cannot select instruction",
            "gcc: fatal error: Killed signal terminated program cc1",
        ]
        with tempfile.TemporaryDirectory() as temporary:
            for index, diagnostic in enumerate(diagnostics):
                for stream in ("stdout", "stderr"):
                    with self.subTest(diagnostic=diagnostic, stream=stream):
                        program = f"import sys; print({diagnostic!r}, file=sys.{stream}); sys.exit(1)"
                        result = audit.run(
                            [sys.executable, "-c", program],
                            Path(temporary),
                            f"driver-{index}-{stream}",
                        )
                        self.assertEqual(result["status"], "crash")

    def test_retained_parity_requires_complete_successful_output(self):
        self.assertFalse(audit.parity({"status": "accepted"}, {"status": "accepted"}))
        self.assertTrue(
            audit.parity(
                {"status": "accepted", "unit_sha256": "a"},
                {"status": "accepted", "unit_sha256": "a"},
            )
        )
        self.assertFalse(
            audit.parity(
                {"status": "accepted", "unit_sha256": "a"},
                {"status": "accepted", "unit_sha256": "b"},
            )
        )
        self.assertFalse(audit.parity({"status": "crash"}, {"status": "crash"}))
        self.assertFalse(audit.parity({"status": "rejected"}, {"status": "rejected"}))
        a = {"status": "rejected", "result": {"diagnostic": "unsupported X"}}
        self.assertTrue(audit.parity(a, a))

    def test_partial_probe_output_cannot_claim_success(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "result.json"
            output.write_text(json.dumps({"status": "accepted"}))
            args = type(
                "Args", (), {"runner": Path("runner"), "target": "target", "timeout": 1}
            )()
            with patch.object(
                audit,
                "run",
                return_value={"status": "tool_error", "stdout": str(output)},
            ):
                result = audit.probe(args, root / "input.i", "gcc", root, "normal")
            self.assertEqual(result["status"], "tool_error")
            self.assertNotIn("unit_sha256", result)

    def test_modified_fixture_cannot_keep_its_pin(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "case.c"
            source.write_text("int x;\n")
            manifest = {
                "headers": [],
                "cases": [{"path": "case.c", "sha256": audit.digest(source)}],
            }
            audit.pinned_files(manifest, root)
            source.write_text("float x;\n")
            with self.assertRaisesRegex(RuntimeError, "pinned hash"):
                audit.pinned_files(manifest, root)

    def test_excluded_case_is_not_an_accepted_frontend(self):
        result = audit.summarize(
            {
                "cases": [
                    {
                        "seed": 7,
                        "eligible": False,
                        "compilers": {"gcc": {"syntax": {"status": "rejected"}}},
                    }
                ]
            }
        )
        self.assertEqual(result["eligible"], 0)
        self.assertEqual(result["accepted_profiles"], 0)
        self.assertEqual(result["excluded"], [7])
        self.assertEqual(result["frontend_failures"], [])

    @unittest.skipUnless(sys.platform == "linux", "process limits are Linux-only")
    def test_process_timeout_is_recorded(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = audit.run(
                [sys.executable, "-c", "import time; time.sleep(30)"],
                Path(temporary),
                "sleep",
                timeout=0.05,
            )
            self.assertEqual(result["status"], "timeout")


if __name__ == "__main__":
    unittest.main()
