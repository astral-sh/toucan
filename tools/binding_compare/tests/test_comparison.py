"""Regression tests for matching independently generated nominal record names."""

import copy
import importlib.util
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[3] / "scripts/compare_bindings.py"
SPEC = importlib.util.spec_from_file_location("compare_bindings", MODULE_PATH)
compare = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(compare)


def record(name, aliases=(), fields=()):
    return {
        "rust_name": name,
        "aliases": list(aliases),
        "kind": "struct",
        "opaque": False,
        "fields": list(fields),
        "excluded_fields": [],
    }


def api(records):
    return {"records": records, "functions": {}, "globals": {}, "aliases": {}}


class ComparisonTests(unittest.TestCase):
    def test_shared_typedef_pairs_anonymous_record_with_named_record(self):
        left = api({"generated": record("generated", ["Public"])})
        right = api({"Public": record("Public")})
        pairs, conflicts = compare.record_pairs(left, right)
        self.assertEqual(pairs, {"generated": "Public"})
        self.assertEqual(conflicts, [])

    def test_mapping_cannot_merge_distinct_types(self):
        left = api({"A": record("A", ["Public"]), "B": record("B", ["Public"])})
        right = api({"Public": record("Public")})
        pairs, conflicts = compare.record_pairs(left, right)
        self.assertEqual(len(pairs), 1)
        self.assertEqual(len(conflicts), 1)

    def test_keyword_escape_requires_counterpart(self):
        self.assertEqual(compare.field_name("type_", {"type"}), "type")
        self.assertEqual(compare.field_name("type_", {"type_"}), "type_")
        self.assertEqual(compare.field_name("ordinary_", {"ordinary"}), "ordinary_")

    def test_missing_and_different_declarations_are_reported(self):
        result = compare.compare_maps(
            {"same": 1, "different": 2, "extra": 3},
            {"same": 1, "different": 4, "missing": 5},
        )
        self.assertEqual(result["equal"], 1)
        self.assertEqual(result["toucan_only"], ["extra"])
        self.assertEqual(result["bindgen_only"], ["missing"])
        self.assertEqual(
            result["different"], {"different": {"toucan": 2, "bindgen": 4}}
        )
        self.assertFalse(compare.matches(result))

    def test_symbol_groups_compare_every_public_name_shape_and_linker_mapping(self):
        shape = {
            "kind": "pointer",
            "value": {
                "mutable": False,
                "pointee": {"kind": "primitive", "value": "i32"},
            },
        }
        for category in ("functions", "globals"):
            item_shape = (
                shape
                if category == "globals"
                else {
                    "abi": "C",
                    "unsafe_": True,
                    "parameters": [shape],
                    "result": {"kind": "unit"},
                    "variadic": False,
                }
            )
            exports = [
                {"rust_name": name, "shape": copy.deepcopy(item_shape)}
                for name in ("first", "second")
            ]
            left = {category: {"symbol": exports}}
            reordered = {category: {"symbol": list(reversed(exports))}}
            groups = compare.export_groups(left, category)
            self.assertTrue(
                compare.matches(
                    compare.compare_maps(
                        groups, compare.export_groups(reordered, category)
                    )
                )
            )
            missing = {category: {"symbol": [exports[1]]}}
            self.assertFalse(
                compare.matches(
                    compare.compare_maps(
                        groups, compare.export_groups(missing, category)
                    )
                )
            )
            different_symbol = {category: {"other_symbol": exports}}
            self.assertFalse(
                compare.matches(
                    compare.compare_maps(
                        groups, compare.export_groups(different_symbol, category)
                    )
                )
            )
            changed = copy.deepcopy(left)
            changed_shape = changed[category]["symbol"][0]["shape"]
            if category == "functions":
                changed_shape = changed_shape["parameters"][0]
            changed_shape["value"]["mutable"] = True
            self.assertFalse(
                compare.matches(
                    compare.compare_maps(
                        groups, compare.export_groups(changed, category)
                    )
                )
            )

    def test_record_pairs_follow_all_shared_export_names(self):
        left = api({name: record(name) for name in ("LA", "LB")})
        right = api({name: record(name) for name in ("RA", "RB")})
        left["globals"] = {
            "symbol": [
                {"rust_name": name, "shape": {"kind": "record", "value": target}}
                for name, target in [("one", "LA"), ("two", "LB")]
            ]
        }
        right["globals"] = {
            "symbol": [
                {"rust_name": name, "shape": {"kind": "record", "value": target}}
                for name, target in [("two", "RB"), ("one", "RA")]
            ]
        }
        self.assertEqual(
            compare.record_pairs(left, right), ({"LA": "RA", "LB": "RB"}, [])
        )

    def test_record_pairing_uses_only_unambiguous_singleton_fallback(self):
        left, right = api({"L": record("L")}), api({"R": record("R")})
        left["globals"] = {
            "symbol": [{"rust_name": "left", "shape": {"kind": "record", "value": "L"}}]
        }
        right["globals"] = {
            "symbol": [
                {"rust_name": "right", "shape": {"kind": "record", "value": "R"}}
            ]
        }
        self.assertEqual(compare.record_pairs(left, right), ({"L": "R"}, []))
        self.assertFalse(
            compare.matches(
                compare.compare_maps(
                    compare.export_groups(left, "globals"),
                    compare.export_groups(right, "globals"),
                )
            )
        )
        left["globals"]["symbol"].append(
            {"rust_name": "another_left", "shape": {"kind": "record", "value": "L"}}
        )
        right["globals"]["symbol"].append(
            {"rust_name": "another_right", "shape": {"kind": "record", "value": "R"}}
        )
        self.assertEqual(compare.record_pairs(left, right), ({}, []))

    def test_analyzer_schema_and_duplicate_name_fail_closed(self):
        for version in (None, 1, 3):
            with self.assertRaisesRegex(
                RuntimeError, "unsupported binding analyzer schema"
            ):
                compare.validate_api({"schema_version": version})
        current = {"schema_version": 2, "functions": {}, "globals": {}}
        compare.validate_api(current)
        current["globals"] = {"symbol": {"rust_name": "old", "shape": {}}}
        with self.assertRaisesRegex(RuntimeError, "expected export list"):
            compare.validate_api(current)
        current["globals"] = {
            "symbol": [
                {"rust_name": "same", "shape": {}},
                {"rust_name": "same", "shape": {}},
            ]
        }
        with self.assertRaisesRegex(RuntimeError, "duplicate public foreign Rust name"):
            compare.validate_api(current)

    def test_probes_preserve_unsigned_values_without_wrapping(self):
        result = compare.parse_probe(
            "constant\tMAX\t340282366920938463463374607431768211455\nrecord\tS\t16\t8\nfield\tS\tp\t8\n"
        )
        self.assertEqual(result["constants"]["MAX"], str(2**128 - 1))
        self.assertEqual(result["records"]["S"], {"size": 16, "alignment": 8})
        self.assertEqual(result["fields"]["S.p"], 8)


if __name__ == "__main__":
    unittest.main()
