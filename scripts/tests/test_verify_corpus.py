"""The independent corpus oracle must account for every generated constant."""

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_corpus as verify


class BooleanProbeTests(unittest.TestCase):
    def generate(self, source, **metadata):
        with patch.dict(verify.PROBES, {"booleans": []}):
            return verify.generate_probes(
                "booleans",
                Path("booleans.h"),
                source,
                {"enum_constants": [], **metadata},
                run_ffi=False,
            )

    def test_boolean_only_headers_require_a_c_type_probe(self):
        c, rust, coverage = self.generate(
            "pub const r#type: ::core::primitive::bool = true;\n",
            renamed_macros={"r#type": "type"},
        )
        self.assertEqual(coverage["integer_constants"], 1)
        self.assertEqual(coverage["boolean_constants"], 1)
        self.assertIn("_Generic((type), _Bool: 1, default: 0)", c)
        self.assertIn("u8::from(b::r#type)", rust)

    def test_unknown_constant_types_and_false_projection_metadata_fail_closed(self):
        with self.assertRaisesRegex(RuntimeError, "coverage is incomplete"):
            self.generate("pub const OK: bool = true;\npub const BAD: f32 = 1.0;\n")
        with self.assertRaisesRegex(RuntimeError, "invalid macro integer metadata"):
            self.generate(
                "pub const OK: bool = true;\n",
                macro_types=[{"rust_name": "OK", "rust_bits": 8, "rust_signed": False}],
            )
        with self.assertRaisesRegex(
            RuntimeError, "Boolean constant has integer projection"
        ):
            self.generate(
                "pub const OK: bool = true;\n",
                enum_constants=[
                    {
                        "c_type": "enum E",
                        "variants": ["OK"],
                        "emitted": [{"rust_name": "OK"}],
                    }
                ],
            )


if __name__ == "__main__":
    unittest.main()
