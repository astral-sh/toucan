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
        self,
        declaration,
        c_type,
        variants,
        rust_type,
        value,
        c_expression_bits=32,
        *,
        rust_name="SELECTED",
        macro_name=None,
        macro_type=None,
        extra_bindings="",
    ):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            header = directory / "api.h"
            header.write_text(declaration)
            fixture = directory / "corpus/ffi/probe.rs"
            fixture.parent.mkdir(parents=True)
            fixture.write_text("fn ffi_test() {}")
            bindings = (
                f"pub const {rust_name}: ::core::primitive::{rust_type} = {value};\n"
                + extra_bindings
            )
            metadata = {
                "enum_constants": [
                    {
                        "c_type": c_type,
                        "variants": variants,
                        "emitted": [
                            {
                                "c_name": "SELECTED",
                                "rust_name": rust_name,
                                "c_expression_bits": c_expression_bits,
                                "c_expression_signed": True,
                            }
                        ],
                    }
                ]
            }
            if macro_name is not None:
                metadata["enum_constants"] = []
                metadata["renamed_macros"] = {rust_name: macro_name}
            if macro_type is not None:
                metadata["enum_constants"] = []
                metadata["macro_types"] = [
                    {
                        "c_name": macro_name or "SELECTED",
                        "rust_name": rust_name,
                        "c_bits": macro_type[0],
                        "c_signed": macro_type[1],
                        "rust_bits": int(rust_type[1:]),
                        "rust_signed": rust_type.startswith("i"),
                    }
                ]
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

    def test_cstr_bytes_are_compared_with_the_original_c_literal(self):
        for byte, matches in [(10, True), (13, False)]:
            expected, actual, coverage = self.compile_probes(
                '#define SELECTED 1\n#define TEXT "a\\nb"\n#define EMPTY ""\n',
                None,
                [],
                "i32",
                1,
                macro_name="SELECTED",
                extra_bindings=(
                    "pub const TEXT: &::core::ffi::CStr = unsafe { "
                    f"::core::ffi::CStr::from_bytes_with_nul_unchecked(&[97, {byte}, 98, 0])"
                    " };\n"
                    "pub const EMPTY: &::core::ffi::CStr = unsafe { "
                    "::core::ffi::CStr::from_bytes_with_nul_unchecked(&[0]) };\n"
                ),
            )
            self.assertEqual(expected == actual, matches)
            self.assertEqual(coverage["string_constants"], 2)

    def test_normalized_macros_check_original_c_values_and_types(self):
        expected, actual, _ = self.compile_probes(
            "#define SELECTED ((_Bool)1)\n", None, [], "u8", 1, macro_name="SELECTED"
        )
        self.assertEqual(expected, actual)
        expected, actual, _ = self.compile_probes(
            "#define SELECTED 5LL\n", None, [], "u32", 5, macro_type=(64, True)
        )
        self.assertEqual(expected, actual)
        for replacement, value in [("-1LL", 4294967295), ("(1LL << 40)", 0)]:
            with self.subTest(replacement=replacement):
                expected, actual, _ = self.compile_probes(
                    f"#define SELECTED {replacement}\n",
                    None,
                    [],
                    "u32",
                    value,
                    macro_type=(64, True),
                )
                self.assertNotEqual(
                    expected["macro_expression_value.SELECTED"],
                    actual["macro_expression_value.SELECTED"],
                )

    def test_renamed_macros_use_their_original_c_names(self):
        for c_name, rust_name in [("self", "__toucan_self_"), ("type", "r#type")]:
            with self.subTest(name=c_name):
                expected, actual, coverage = self.compile_probes(
                    f"#define {c_name} 7\n",
                    None,
                    [],
                    "i32",
                    7,
                    rust_name=rust_name,
                    macro_name=c_name,
                )
                self.assertEqual(expected, actual)
                self.assertEqual(coverage["enum_constants"], 0)

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
            (
                "enum { NEGATIVE = -1, SELECTED = 3 };\n#define NEGATIVE 4294967295U\n",
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
