"""The strict gate must retain exploratory differences and infrastructure failures."""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "audit_c_testsuite", ROOT / "scripts/audit_c_testsuite.py"
)
assert SPEC and SPEC.loader
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


def case(name: str, *, strict: bool, accepts: bool) -> dict:
    return {
        "name": name,
        "both_accept": {"c11": True, "gnu11": True, "strict_c11": strict},
        "eligible": True,
        "difference": not accepts,
        "toucan": {"exit_code": 0 if accepts else 1, "timeout": False},
        "tool_failure": False,
        "oracle_pipeline_failure": False,
    }


class StrictGateTests(unittest.TestCase):
    def test_exploratory_difference_remains_visible_without_failing_strict_gate(self):
        cases = [
            case("strict.c", strict=True, accepts=True),
            case("extension.c", strict=False, accepts=False),
        ]
        summary = AUDIT.summarize(cases, [])
        self.assertEqual(summary["differences"], ["extension.c"])
        self.assertEqual(summary["strict_eligible_count"], 1)
        self.assertEqual(summary["strict_toucan_accepted"], 1)
        self.assertEqual(summary["strict_differences"], [])
        self.assertFalse(AUDIT.audit_failed(summary, fail_on_strict_difference=True))
        self.assertTrue(AUDIT.audit_failed(summary, fail_on_difference=True))

    def test_new_strict_rejection_fails_without_a_baseline_or_case_exception(self):
        summary = AUDIT.summarize(
            [case("previously-unseen.c", strict=True, accepts=False)], []
        )
        self.assertEqual(summary["strict_differences"], ["previously-unseen.c"])
        self.assertEqual(summary["strict_toucan_accepted"], 0)
        self.assertTrue(AUDIT.audit_failed(summary, fail_on_strict_difference=True))
        self.assertFalse(AUDIT.audit_failed(summary))

    def test_infrastructure_failure_is_fatal_even_outside_the_strict_subset(self):
        for field in ["tool_failure", "oracle_pipeline_failure"]:
            with self.subTest(field=field):
                failed = case("extension.c", strict=False, accepts=False)
                failed[field] = True
                summary = AUDIT.summarize([failed], [])
                self.assertTrue(AUDIT.audit_failed(summary))
                self.assertTrue(
                    AUDIT.audit_failed(summary, fail_on_strict_difference=True)
                )
        summary = AUDIT.summarize([], ["toucan"])
        self.assertTrue(AUDIT.audit_failed(summary, fail_on_strict_difference=True))

    def test_command_line_accepts_both_gate_flags(self):
        with patch.object(
            sys,
            "argv",
            [
                "audit_c_testsuite.py",
                "--fail-on-strict-difference",
                "--fail-on-difference",
            ],
        ):
            arguments = AUDIT.arguments()
        self.assertTrue(arguments.fail_on_strict_difference)
        self.assertTrue(arguments.fail_on_difference)


if __name__ == "__main__":
    unittest.main()
