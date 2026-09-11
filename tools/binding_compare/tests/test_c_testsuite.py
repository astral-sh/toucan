"""The strict gate must retain exploratory differences and infrastructure failures."""

from __future__ import annotations

import argparse
import contextlib
import importlib.util
import io
import json
import shutil
import sys
import tempfile
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
        "both_accept": {mode: True for mode in AUDIT.MODES} | {"strict_c11": strict},
        "eligible": True,
        "difference": not accepts,
        "toucan": {"exit_code": 0 if accepts else 1, "timeout": False},
        "tool_failure": False,
        "oracle_pipeline_failure": False,
    }


class StrictGateTests(unittest.TestCase):
    def test_shared_diagnostics_are_fingerprinted_and_changes_fail_the_audit(self):
        for mutate in (False, True):
            with (
                self.subTest(mutate=mutate),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = Path(temporary)
                driver = root / "audit_c_testsuite.py"
                helper = root / "compiler_diagnostics.py"
                for path in (driver, helper):
                    shutil.copyfile(ROOT / "scripts" / path.name, path)
                original_hash = AUDIT.digest(helper)
                inputs = root / "inputs"
                inputs.mkdir()
                (inputs / "case.c").write_text("int value;\n")
                manifest = root / "manifest.json"
                manifest.write_text(
                    json.dumps({"source_count": 1, "tests_directory": "."})
                )
                output = root / "output"

                def run(command, directory, stem, timeout):
                    stdout = directory / f"{stem}.stdout"
                    stdout.write_text(
                        "clang" if stem == "clang-version" else "compiler"
                    )
                    return {"exit_code": 0, "timeout": False, "stdout": str(stdout)}

                def audit(*args):
                    if mutate:
                        helper.write_text(
                            "def has_crash_diagnostic(*streams): return False\n"
                        )
                    return case("case.c", strict=True, accepts=True)

                with (
                    patch.object(
                        sys,
                        "argv",
                        [
                            "audit_c_testsuite.py",
                            "--cache",
                            str(root / "cache"),
                            "--output",
                            str(output),
                            "--target",
                            "x86_64-unknown-linux-gnu",
                        ],
                    ),
                    patch.object(AUDIT, "__file__", str(driver)),
                    patch.object(AUDIT, "MANIFEST", manifest),
                    patch.object(
                        AUDIT, "prepare_archive", return_value=root / "archive"
                    ),
                    patch.object(AUDIT, "extract_sources", return_value=inputs),
                    patch.object(AUDIT, "executable", return_value=sys.executable),
                    patch.object(AUDIT, "run", side_effect=run),
                    patch.object(AUDIT, "audit", side_effect=audit),
                    contextlib.redirect_stdout(io.StringIO()),
                ):
                    result = AUDIT.main()
                report = json.loads((output / "evidence.json").read_text())
                self.assertEqual(
                    report["tools"]["compiler_diagnostics"],
                    {
                        "path": str(helper),
                        "sha256": original_hash,
                    },
                )
                self.assertEqual(result, int(mutate))
                self.assertEqual(
                    report["summary"]["changed_tools"],
                    ["compiler_diagnostics"] if mutate else [],
                )

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


class NativeSourceGateTests(unittest.TestCase):
    def native_case(self, status, *, strict=True, eligible=True):
        result = case("native.c", strict=strict, accepts=True)
        result["eligible"] = eligible
        result["native_source"] = {
            "status": status,
            "difference": eligible and status != "accepted",
        }
        return result

    def test_failed_native_route_fails_strict_gate_while_legacy_route_passes(self):
        for status in ["preprocessing_rejected", "analysis_rejected"]:
            with self.subTest(status=status):
                summary = AUDIT.summarize([self.native_case(status)], [])
                self.assertEqual(summary["strict_toucan_accepted"], 1)
                self.assertEqual(summary["strict_differences"], [])
                self.assertEqual(summary["native_source"]["strict_accepted"], 0)
                self.assertEqual(summary["native_source"][status], ["native.c"])
                self.assertTrue(
                    AUDIT.audit_failed(summary, fail_on_strict_difference=True)
                )
                self.assertFalse(AUDIT.audit_failed(summary))

    def test_exclusions_and_exploratory_rejections_do_not_become_strict_failures(self):
        for strict, eligible in [(False, True), (True, False)]:
            summary = AUDIT.summarize(
                [
                    self.native_case(
                        "analysis_rejected", strict=strict, eligible=eligible
                    )
                ],
                [],
            )
            self.assertEqual(summary["native_source"]["strict_eligible_count"], 0)
            self.assertFalse(
                AUDIT.audit_failed(summary, fail_on_strict_difference=True)
            )
        summary = AUDIT.summarize(
            [self.native_case("analysis_rejected", strict=False)], []
        )
        self.assertTrue(AUDIT.audit_failed(summary, fail_on_difference=True))

    def test_native_infrastructure_and_input_changes_always_fail(self):
        for status in ["tool_failure", "input_changed"]:
            summary = AUDIT.summarize(
                [self.native_case(status, strict=False, eligible=False)], []
            )
            self.assertTrue(AUDIT.audit_failed(summary))

    def test_explicit_opt_in_preserves_legacy_summary(self):
        summary = AUDIT.summarize([case("legacy.c", strict=True, accepts=True)], [])
        self.assertNotIn("native_source", summary)
        with patch.object(
            sys, "argv", ["audit", "--native-source", "--native-probe", "/probe"]
        ):
            args = AUDIT.arguments()
        self.assertTrue(args.native_source)
        self.assertEqual(args.native_probe, "/probe")

    def test_source_selection_flags_preserve_order_and_reject_other_options(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = argparse.Namespace(
                compiler="gcc",
                cc_arg=["-DX=1", "-U", "X"],
                gcc_arg=["-DX=2", "-I", temporary],
            )
            self.assertEqual(
                AUDIT.native_definitions(args), [("X", "1"), ("X", None), ("X", "2")]
            )
            for flags in [
                ["-m32"],
                ["-isystem", temporary],
                ["-Irelative"],
                ["-D"],
                ["-include", "header.h"],
                ["@args"],
            ]:
                args.gcc_arg = flags
                with self.subTest(flags=flags), self.assertRaises(RuntimeError):
                    AUDIT.native_definitions(args)

    def test_include_search_does_not_drop_unknown_entries(self):
        with tempfile.TemporaryDirectory() as temporary:
            start = '#include "..." search starts here:\n#include <...> search starts here:\n'
            self.assertEqual(
                AUDIT.compiler_include_dirs(
                    start + temporary + "\nEnd of search list.\n"
                ),
                [temporary],
            )
            for entry in [
                temporary + " (framework directory)",
                "/missing/native/headers",
                "relative",
            ]:
                with self.subTest(entry=entry), self.assertRaises(RuntimeError):
                    AUDIT.compiler_include_dirs(
                        start + entry + "\nEnd of search list.\n"
                    )
            with self.assertRaises(RuntimeError):
                AUDIT.compiler_include_dirs(start + temporary)

    def test_macro_differences_do_not_replace_the_shipped_profile(self):
        reference = AUDIT.compiler_definitions(
            "#define EMPTY\n#define VERSION 18\n#define ONLY_C(x) x\n"
        )
        shipped = {"EMPTY": "", "VERSION": "17", "ONLY_T": "1"}
        differences = AUDIT.macro_differences(reference, shipped)
        self.assertEqual(
            differences,
            {
                "compiler_only": {"ONLY_C(x)": "x"},
                "toucan_only": {"ONLY_T": "1"},
                "different": {"VERSION": {"compiler": "18", "toucan": "17"}},
            },
        )
        self.assertEqual(shipped["VERSION"], "17")

    def test_probe_protocol_and_rejections_cannot_count_as_acceptance(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            source = directory / "source.c"
            source.write_text("int valid;\n")
            args = argparse.Namespace(
                native_probe="/probe",
                target="x86_64-unknown-linux-gnu",
                compiler="gcc",
                dialect="c11",
                timeout=1,
                native_configuration={
                    "include_dirs": [],
                    "definitions": [],
                    "probe_configuration": {"definitions": {}},
                },
            )
            scenarios = [
                (0, '{"status":"rejected","stage":"preprocessing"}', "tool_failure"),
                (
                    1,
                    '{"status":"rejected","stage":"preprocessing"}',
                    "preprocessing_rejected",
                ),
                (0, "{", "tool_failure"),
                (0, "[]", "tool_failure"),
                (0, None, "tool_failure"),
                (0, '{"status":"preprocessed"}', "tool_failure"),
            ]
            for exit_code, payload, expected in scenarios:

                def command(
                    argv, cwd, stem, timeout, *, payload=payload, exit_code=exit_code
                ):
                    stdout = directory / "probe.stdout"
                    stdout.unlink(missing_ok=True)
                    if payload is not None:
                        stdout.write_text(payload)
                    return {
                        "exit_code": exit_code,
                        "timeout": False,
                        "crash_diagnostic": False,
                        "stdout": str(stdout),
                    }

                with self.subTest(payload=payload), patch.object(AUDIT, "run", command):
                    native = AUDIT.audit_native(source, args, directory)
                self.assertEqual(native["status"], expected)
                tested = self.native_case(native["status"])
                self.assertTrue(
                    AUDIT.audit_failed(
                        AUDIT.summarize([tested], []), fail_on_strict_difference=True
                    )
                )

    def test_final_input_sweep_synchronizes_case_sidecars(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            source = output / "native.c"
            source.write_text("int original;\n")
            item = self.native_case("accepted")
            item.update(source=str(source), source_sha256=AUDIT.digest(source))
            item["native_source"]["dependencies_before"] = {
                str(source): AUDIT.digest(source)
            }
            directory = output / "cases/native"
            directory.mkdir(parents=True)
            AUDIT.write_json(directory / "result.json", item)
            source.write_text("int changed;\n")
            inventory, changed = AUDIT.verify_native_inputs([item], output)
            self.assertEqual(changed, [str(source)])
            self.assertEqual(inventory[str(source)], item["source_sha256"])
            self.assertEqual(item["native_source"]["status"], "input_changed")
            self.assertEqual(json.loads((directory / "result.json").read_text()), item)
            self.assertTrue(AUDIT.audit_failed(AUDIT.summarize([item], [])))

    def test_analysis_rejection_and_header_changes_after_preprocessing_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            source = directory / "source.c"
            header = directory / "header.h"
            source.write_text('#include "header.h"\n')
            args = argparse.Namespace(
                native_configuration={"probe_configuration": {"definitions": {}}}
            )
            for mutate in [False, True]:
                header.write_text("int value;\n")

                def probe(args, source, directory, operation, *, mutate=mutate):
                    command = {
                        "exit_code": 0 if operation == "preprocess" else 1,
                        "timeout": False,
                        "crash_diagnostic": False,
                    }
                    if operation == "preprocess":
                        return (
                            command,
                            {
                                "status": "preprocessed",
                                "definitions": {},
                                "dependencies": [str(source), str(header)],
                            },
                            directory / "native.i",
                        )
                    if mutate:
                        header.write_text("int changed;\n")
                    return (
                        command,
                        {"status": "rejected", "stage": "analysis"},
                        directory / "declarations",
                    )

                with (
                    self.subTest(mutate=mutate),
                    patch.object(AUDIT, "run_probe", probe),
                ):
                    native = AUDIT.audit_native(source, args, directory)
                self.assertEqual(
                    native["status"], "input_changed" if mutate else "analysis_rejected"
                )
                self.assertTrue(
                    AUDIT.audit_failed(
                        AUDIT.summarize([self.native_case(native["status"])], []),
                        fail_on_strict_difference=True,
                    )
                )


if __name__ == "__main__":
    unittest.main()
