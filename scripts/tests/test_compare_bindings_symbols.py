"""Keep linker-name normalization narrower than Rust public-name comparison."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from compare_bindings import canonical_elf_symbols, export_groups, record_pairs


class ElfLinkNameTests(unittest.TestCase):
    def test_elf_marker_maps_to_the_same_symbol_without_changing_rust_names(self):
        api = {
            "functions": {
                "\x01library_call": [
                    {
                        "rust_name": "public_call",
                        "shape": {"kind": "record", "value": "R"},
                    }
                ]
            },
            "globals": {
                "\x01data": [
                    {
                        "rust_name": "public_data",
                        "shape": {"kind": "primitive", "value": "u8"},
                    }
                ]
            },
        }
        self.assertEqual(
            canonical_elf_symbols(api, "x86_64-unknown-linux-gnu"),
            {"functions": 1, "globals": 1},
        )
        self.assertEqual(set(api["functions"]), {"library_call"})
        self.assertEqual(set(api["globals"]), {"data"})
        self.assertEqual(
            set(export_groups(api, "functions")["library_call"]), {"public_call"}
        )

    def test_symbol_pairing_recovers_opaque_record_type_correspondence(self):
        shape = lambda name: {"kind": "record", "value": name}
        a = {
            "records": {
                "Owner_T": {"rust_name": "Owner_T", "aliases": [], "fields": []}
            },
            "aliases": {},
            "functions": {"call": [{"rust_name": "call", "shape": shape("Owner_T")}]},
            "globals": {},
        }
        b = {
            "records": {"T": {"rust_name": "T", "aliases": [], "fields": []}},
            "aliases": {},
            "functions": {"\x01call": [{"rust_name": "call", "shape": shape("T")}]},
            "globals": {},
        }
        self.assertEqual(record_pairs(a, b)[0], {})
        canonical_elf_symbols(b, "x86_64-unknown-linux-gnu")
        self.assertEqual(record_pairs(a, b)[0], {"Owner_T": "T"})

    def test_non_elf_markers_remain_distinct(self):
        api = {"functions": {"\x01f": [{"rust_name": "f", "shape": {}}]}, "globals": {}}
        self.assertEqual(
            canonical_elf_symbols(api, "x86_64-apple-darwin"),
            {"functions": 0, "globals": 0},
        )
        self.assertIn("\x01f", api["functions"])

    def test_duplicate_and_empty_elf_names_fail_closed(self):
        for names in ({"f": [], "\x01f": []}, {"\x01": []}):
            with self.subTest(names=names), self.assertRaises(RuntimeError):
                canonical_elf_symbols(
                    {"functions": names, "globals": {}}, "aarch64-unknown-linux-musl"
                )


if __name__ == "__main__":
    unittest.main()
