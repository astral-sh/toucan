"""Ensure the regression gate rejects differences outside its C-checked exceptions."""

import copy
import importlib.util
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[3] / "scripts/verify_equivalence.py"
SPEC = importlib.util.spec_from_file_location("verify_equivalence", MODULE_PATH)
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


def report():
    return {
        "comparisons": {
            category: {"toucan_only": [], "bindgen_only": [], "different": {}}
            for category in (
                "functions",
                "globals",
                "constants",
                "aliases",
                "record_shapes",
                "native_records",
                "native_fields",
                "native_constants",
            )
        },
        "inventory": {
            side: {"records": {}, "aliases": {}} for side in ("toucan", "bindgen")
        },
        "excluded_storage_fields": {"toucan": {}, "bindgen": {}},
        "record_name_mapping": {},
        "record_mapping_conflicts": [],
        "unsupported": {"toucan": [], "bindgen": []},
        "inputs_unchanged": True,
        "native_observations": {"toucan": {}, "bindgen": {}},
    }


class EquivalenceGateTests(unittest.TestCase):
    def test_enum_exception_requires_a_known_enumerator(self):
        value = report()
        value["comparisons"]["constants"]["different"]["VALUE"] = {
            "toucan": gate.primitive("i32"),
            "bindgen": gate.primitive("u32"),
        }
        project = {"name": "zstd", "allowlist": ["*"]}
        accepted, unexpected = gate.classify(value, project, {"VALUE"})
        self.assertEqual(len(accepted), 1)
        self.assertFalse(unexpected)
        accepted, unexpected = gate.classify(value, project, set())
        self.assertFalse(accepted)
        self.assertEqual(len(unexpected), 1)

    def test_sentinel_exception_requires_exact_width_signedness_and_value(self):
        value = report()
        item = "ZSTD_CONTENTSIZE_ERROR"
        value["comparisons"]["constants"]["different"][item] = {
            "toucan": gate.primitive("u64"),
            "bindgen": gate.primitive("i32"),
        }
        value["comparisons"]["native_constants"]["different"][item] = {
            "toucan": str((1 << 64) - 2),
            "bindgen": "-2",
        }
        project = {"name": "zstd", "allowlist": ["ZSTD*"]}
        self.assertFalse(gate.classify(value, project, set())[1])
        value["comparisons"]["native_constants"]["different"][item]["toucan"] = "0"
        self.assertEqual(len(gate.classify(value, project, set())[1]), 2)

    def test_layout_offset_function_and_global_differences_are_never_accepted(self):
        value = report()
        for category in ("native_records", "native_fields", "functions", "globals"):
            value["comparisons"][category]["different"]["item"] = {
                "toucan": 1,
                "bindgen": 2,
            }
        self.assertEqual(len(gate.classify(value, {"name": "sqlite"}, set())[1]), 4)

    def test_only_named_extra_constants_are_accepted(self):
        value = report()
        for category in ("constants", "native_constants"):
            value["comparisons"][category]["toucan_only"] = [
                "ZSTD_VERSION_STRING",
                "NEW_CONSTANT",
            ]
        accepted, unexpected = gate.classify(value, {"name": "zstd"}, set())
        self.assertEqual({item["name"] for item in accepted}, {"ZSTD_VERSION_STRING"})
        self.assertEqual({item["name"] for item in unexpected}, {"NEW_CONSTANT"})

    def test_storage_exception_requires_exact_fields_and_record(self):
        value = report()
        value["excluded_storage_fields"]["toucan"]["git_commit_create_options"] = [
            "__toucan_bits_1",
            "__toucan_padding_2",
        ]
        self.assertFalse(gate.classify(value, {"name": "libgit2"}, set())[1])
        value["excluded_storage_fields"]["toucan"]["git_commit_create_options"].append(
            "unvalidated_storage"
        )
        self.assertTrue(gate.classify(value, {"name": "libgit2"}, set())[1])

    def test_va_list_storage_requires_independent_c_validation(self):
        value = report()
        value["comparisons"]["native_fields"]["toucan_only"] = [
            "__builtin_va_list_record.__stack"
        ]
        self.assertTrue(gate.classify(value, {"name": "sqlite"}, set())[1])
        value["c_validated_va_list"] = True
        self.assertFalse(gate.classify(value, {"name": "sqlite"}, set())[1])
        value["comparisons"]["native_fields"]["toucan_only"].append(
            "__builtin_va_list_record.unvalidated_field"
        )
        self.assertTrue(gate.classify(value, {"name": "sqlite"}, set())[1])

    def test_sqlite_exception_does_not_hide_another_field_change(self):
        callback = {
            "abi": "C",
            "unsafe_": True,
            "parameters": [],
            "result": {"kind": "unit"},
            "variadic": False,
        }
        outer = {
            "abi": "C",
            "unsafe_": True,
            "variadic": False,
            "parameters": [
                {
                    "kind": "pointer",
                    "value": {
                        "mutable": True,
                        "pointee": {"kind": "record", "value": "sqlite3_vfs"},
                    },
                },
                {
                    "kind": "pointer",
                    "value": {"mutable": True, "pointee": gate.primitive("void")},
                },
                {
                    "kind": "pointer",
                    "value": {"mutable": False, "pointee": gate.primitive("i8")},
                },
            ],
            "result": {
                "kind": "nullable",
                "value": {"kind": "function", "value": callback},
            },
        }
        left = {
            "fields": [
                {
                    "name": "xDlSym",
                    "shape": {
                        "kind": "nullable",
                        "value": {"kind": "function", "value": outer},
                    },
                },
                {"name": "iVersion", "shape": gate.primitive("i32")},
            ]
        }
        right = copy.deepcopy(left)
        right["fields"][0]["shape"]["value"]["value"]["result"]["value"]["value"][
            "parameters"
        ] = copy.deepcopy(outer["parameters"])
        self.assertTrue(
            gate.sqlite_callback_difference({"toucan": left, "bindgen": right})
        )
        right["fields"][1]["shape"] = gate.primitive("u64")
        self.assertFalse(
            gate.sqlite_callback_difference({"toucan": left, "bindgen": right})
        )

    def test_selected_alias_must_match_a_corresponding_record(self):
        value = report()
        value["comparisons"]["aliases"]["toucan_only"] = ["Public"]
        value["inventory"]["toucan"]["aliases"]["Public"] = {
            "kind": "record",
            "value": "Anonymous",
        }
        value["inventory"]["bindgen"]["records"]["Public"] = {
            "rust_name": "Public",
            "aliases": [],
        }
        value["record_name_mapping"] = {"Anonymous": "Public"}
        self.assertFalse(
            gate.classify(value, {"name": "zlib", "allowlist": ["Public"]}, set())[1]
        )
        value["record_name_mapping"] = {}
        self.assertTrue(
            gate.classify(value, {"name": "zlib", "allowlist": ["Public"]}, set())[1]
        )


if __name__ == "__main__":
    unittest.main()
