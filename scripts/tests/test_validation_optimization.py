"""Validators must refuse to report success when Python removes their checks."""

import ast
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class Optimization(unittest.TestCase):
    def test_assertion_based_validators_fail_before_running_under_optimization(self):
        candidates = [
            *(ROOT / "scripts").glob("*.py"),
            *(ROOT / "corpus").rglob("*.py"),
        ]
        validators = [
            p
            for p in candidates
            if any(
                isinstance(n, ast.Assert) for n in ast.walk(ast.parse(p.read_text()))
            )
        ]
        self.assertTrue(validators)
        # Also cover the public entry point whose checks import these modules.
        validators.append(ROOT / "scripts/verify_astral_optin.py")
        environment = os.environ.copy()
        environment.pop("PYTHONOPTIMIZE", None)
        environment.pop("TOUCAN_GIT_TOKEN", None)
        with tempfile.TemporaryDirectory() as directory:
            for script in validators:
                for flags, extra_env in [
                    ([], {}),
                    (["-O"], {}),
                    ([], {"PYTHONOPTIMIZE": "2"}),
                ]:
                    with self.subTest(
                        script=str(script.relative_to(ROOT)),
                        flags=flags,
                        environment=extra_env,
                    ):
                        result = subprocess.run(
                            [sys.executable, "-B", *flags, str(script), "--help"],
                            env=environment | extra_env,
                            cwd=directory,
                            capture_output=True,
                            text=True,
                            timeout=10,
                            check=False,
                        )
                        if flags or extra_env:
                            self.assertNotEqual(result.returncode, 0)
                            self.assertIn(
                                "unset PYTHONOPTIMIZE and omit -O", result.stderr
                            )
                            self.assertNotIn("usage:", result.stdout)
                        else:
                            self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertEqual(list(Path(directory).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
