"""Native regressions for the C oracle's enum constant representation checks."""

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

MODULE_PATH = Path(__file__).resolve().parents[3] / "scripts/verify_corpus.py"
SPEC = importlib.util.spec_from_file_location("verify_corpus", MODULE_PATH)
probes = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probes)


class EnumProbeTests(unittest.TestCase):
    def compile_probes(
        self, declaration, c_type, variants, rust_type, value, c_expression_bits=32
    ):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            header = directory / "api.h"
            header.write_text(declaration)
            fixture = directory / "corpus/ffi/probe.rs"
            fixture.parent.mkdir(parents=True)
            fixture.write_text("fn ffi_test() {}")
            bindings = (
                f"pub const SELECTED: ::core::primitive::{rust_type} = {value};\n"
            )
            metadata = {
                "enum_constants": [
                    {
                        "c_type": c_type,
                        "variants": variants,
                        "emitted": [
                            {
                                "c_name": "SELECTED",
                                "rust_name": "SELECTED",
                                "c_expression_bits": c_expression_bits,
                                "c_expression_signed": True,
                            }
                        ],
                    }
                ]
            }
            with (
                patch.object(probes, "ROOT", directory),
                patch.dict(probes.PROBES, {"probe": []}),
            ):
                c, rust, coverage = probes.generate_probes(
                    "probe", header, bindings, metadata
                )
            (directory / "bindings.rs").write_text(bindings)
            (directory / "probe.c").write_text(c)
            (directory / "probe.rs").write_text(rust)
            outputs = []
            for compiler, arguments, source in [
                (os.environ.get("CC", "cc"), ["-std=c11"], "probe.c"),
                (os.environ.get("RUSTC", "rustc"), ["--edition=2024"], "probe.rs"),
            ]:
                executable = directory / f"{source}.exe"
                subprocess.run(
                    [
                        compiler,
                        *arguments,
                        str(directory / source),
                        "-o",
                        str(executable),
                    ],
                    check=True,
                    capture_output=True,
                    text=True,
                )
                outputs.append(
                    probes.parse_output(
                        subprocess.check_output([executable], text=True)
                    )
                )
            return *outputs, coverage

    def test_named_and_anonymous_enum_representations_match_c(self):
        cases = [
            ("enum Named { SELECTED = 3 };", "enum Named", ["SELECTED"], "u32", 3),
            ("typedef enum { SELECTED = -2 } Named;", "Named", ["SELECTED"], "i32", -2),
            (
                "enum { NEGATIVE = -1, SELECTED = 3 };",
                None,
                ["NEGATIVE", "SELECTED"],
                "i32",
                3,
            ),
        ]
        for case in cases:
            with self.subTest(declaration=case[0]):
                expected, actual, coverage = self.compile_probes(*case)
                self.assertEqual(expected, actual)
                self.assertEqual(coverage["enum_constants"], 1)
                self.assertEqual(
                    coverage["anonymous_enum_projections"], int(case[1] is None)
                )
                self.assertIn("enum_expression_signed.SELECTED", expected)

    def test_c_oracle_rejects_the_wrong_enum_constant_type(self):
        expected, actual, _ = self.compile_probes(
            "enum Named { SELECTED = 3 };", "enum Named", ["SELECTED"], "i32", 3
        )
        self.assertEqual(expected["constant.SELECTED"], actual["constant.SELECTED"])
        self.assertNotEqual(
            expected["constant_signed.SELECTED"], actual["constant_signed.SELECTED"]
        )
        self.assertEqual(expected["enum_expression_signed.SELECTED"], "1")

    def test_c_oracle_rejects_the_wrong_original_expression_type(self):
        expected, actual, _ = self.compile_probes(
            "enum Named { SELECTED = 3 };",
            "enum Named",
            ["SELECTED"],
            "u32",
            3,
            c_expression_bits=64,
        )
        self.assertEqual(
            expected["constant_size.SELECTED"], actual["constant_size.SELECTED"]
        )
        self.assertNotEqual(
            expected["enum_expression_size.SELECTED"],
            actual["enum_expression_size.SELECTED"],
        )


if __name__ == "__main__":
    unittest.main()
