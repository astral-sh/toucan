"""Regression tests for matching independently generated nominal record names."""

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

    def test_probes_preserve_unsigned_values_without_wrapping(self):
        result = compare.parse_probe(
            "constant\tMAX\t340282366920938463463374607431768211455\nrecord\tS\t16\t8\nfield\tS\tp\t8\n"
        )
        self.assertEqual(result["constants"]["MAX"], str(2**128 - 1))
        self.assertEqual(result["records"]["S"], {"size": 16, "alignment": 8})
        self.assertEqual(result["fields"]["S.p"], 8)


if __name__ == "__main__":
    unittest.main()
