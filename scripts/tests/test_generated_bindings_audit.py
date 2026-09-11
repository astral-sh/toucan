"""The native oracle must detect frontend and output failures, not just finish."""

import argparse
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import audit_generated_bindings as audit


class CompilerResults(unittest.TestCase):
    def test_driver_crash_cannot_satisfy_either_expected_result(self):
        # Apple Clang 17's driver reported this child crash with exit 1 (PR148).
        diagnostic = (
            "clang: error: unable to execute command: Segmentation fault: 11\n"
            "clang: error: clang frontend command failed due to signal "
            "(use -v to see invocation)\n"
        )
        with tempfile.TemporaryDirectory() as temporary:
            for stream in ("stdout", "stderr"):
                for code in (0, 1):
                    with self.subTest(stream=stream, code=code):
                        result = audit.run(
                            [
                                sys.executable,
                                "-c",
                                f"import sys; print({diagnostic!r}, file=sys.{stream}); sys.exit({code})",
                            ],
                            Path(temporary),
                            "driver",
                            5,
                        )
                        self.assertFalse(audit.accepted(result))
                        for expected in (True, False):
                            with self.assertRaisesRegex(RuntimeError, "tool failure"):
                                audit.require_result(result, expected)

    def test_ordinary_diagnostics_remain_source_rejections(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = audit.run(
                [
                    sys.executable,
                    "-c",
                    "import sys; print('error: static assertion failed', file=sys.stderr); sys.exit(1)",
                ],
                Path(temporary),
                "rejection",
                5,
            )
            audit.require_result(result, False)


@unittest.skipUnless(
    os.environ.get("TOUCAN_ORACLE_BINARY"),
    "requires a built Toucan and native GCC/Clang/Rust",
)
class GeneratedBindingsAuditTests(unittest.TestCase):
    def test_bad_frontends_cannot_pass(self):
        binary = str(Path(os.environ["TOUCAN_ORACLE_BINARY"]).resolve())
        for fault, expected in (
            ("accept-invalid", "expected rejection"),
            ("wrong-value", "C/Rust values or layouts differ"),
            ("invalid-rust", "expected acceptance"),
        ):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                wrapper = root / "frontend"
                wrapper.write_text(f"""#!{sys.executable}
import pathlib, re, subprocess, sys
args = sys.argv[1:]
if {fault!r} == 'accept-invalid' and args[0] == 'check' and args[-1] != 'input.h':
    sys.exit(0)
result = subprocess.run([{binary!r}, *args])
if result.returncode == 0 and args[0] == 'bindgen':
    path = pathlib.Path(args[args.index('--output') + 1])
    source = path.read_text()
    if {fault!r} == 'wrong-value':
        source, count = re.subn(r'(pub const VALUE:[^\\n]*=)[^;]+;', r'\\g<1>0;', source)
        assert count == 1
    else:
        source += '\\nthis is invalid Rust;\\n'
    path.write_text(source)
sys.exit(result.returncode)
""")
                wrapper.chmod(0o755)
                args = argparse.Namespace(
                    toucan=wrapper,
                    output=root / "results",
                    seed=20260910,
                    count=1,
                    gcc=os.environ.get("TOUCAN_GCC", "gcc"),
                    clang=os.environ.get("TOUCAN_CLANG", "clang"),
                    toolchain=os.environ.get("TOUCAN_ORACLE_TOOLCHAIN"),
                    timeout=30,
                )
                with self.assertRaisesRegex(RuntimeError, expected):
                    audit.audit(args)
                report = json.loads((args.output / "summary.json").read_text())
                self.assertEqual(report["status"], "failed")
                self.assertIn(expected, report["error"])


if __name__ == "__main__":
    unittest.main()
