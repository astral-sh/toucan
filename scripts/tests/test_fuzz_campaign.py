"""Reproducer preservation and failure classification for sustained fuzzing."""

import hashlib
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import run_fuzz_campaign as campaign


class FuzzCampaignTests(unittest.TestCase):
    def test_stale_profile_override_fails(self):
        with patch.object(campaign, "query_profile", return_value=(15, 0, 0)):
            self.assertEqual(campaign.compiled_profiles("fuzzer"), 15)
            self.assertEqual(campaign.compiled_profiles("fuzzer", 15), 15)
            with self.assertRaisesRegex(ValueError, "compiled harness has 15"):
                campaign.compiled_profiles("fuzzer", 11)

    @unittest.skipUnless(
        os.environ.get("TOUCAN_FUZZ_BINARY"), "requires a built profile-aware harness"
    )
    def test_compiled_harness_covers_every_padded_profile_and_mode(self):
        binary = Path(os.environ["TOUCAN_FUZZ_BINARY"]).resolve()
        count = campaign.compiled_profiles(binary)
        seeds = campaign.seed_profiles(b"int value;", count, modes=8)
        actual = {campaign.query_profile(binary, seed) for seed in seeds}
        self.assertEqual(
            actual, {(count, p, m) for p in range(count) for m in range(8)}
        )
        with self.assertRaises(ValueError):
            campaign.compiled_profiles(binary, count - 1)

    def test_parser_padding_covers_settings_with_only_trailing_whitespace(self):
        for source in (
            b"",
            b'# 1 "header.h"\nint x;',
            'char*s="é𝄞";'.encode(),
            b"x" * 63,
        ):
            seeds = list(campaign.seed_parser_settings(source))
            self.assertEqual(len(seeds), 64)
            self.assertEqual(
                {
                    (
                        sum(seed) & 3,
                        (sum(seed) >> 2) & 3,
                        bool(sum(seed) & 16),
                        bool(sum(seed) & 32),
                    )
                    for seed in seeds
                },
                {
                    (flavor, standard, gnu, msvc)
                    for flavor in range(4)
                    for standard in range(4)
                    for gnu in (False, True)
                    for msvc in (False, True)
                },
            )
            for seed in seeds:
                self.assertTrue(seed.startswith(source))
                self.assertTrue(set(seed[len(source) :]) <= {9, 10})

    def test_source_manifest_records_new_files_and_omits_removed_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "tracked.rs").write_bytes(b"tracked")
            (root / "new.rs").write_bytes(b"new")
            with patch.object(
                campaign, "capture", return_value="tracked.rs\0deleted.rs\0new.rs\0"
            ):
                manifest = campaign.source_manifest(root)
            self.assertEqual(set(manifest), {"tracked.rs", "new.rs"})
            self.assertEqual(manifest["new.rs"], hashlib.sha256(b"new").hexdigest())

    def test_parser_source_scope_tracks_build_inputs_separately_from_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            names = (
                "Cargo.lock",
                "crates/toucan_parser/src/parser/expression.rs",
                "fuzz/fuzz_targets/parser.rs",
                "fuzz/Cargo.toml",
                "benchmarks/results.json",
                "README.md",
            )
            for name in names:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"input")
            with patch.object(campaign, "capture", return_value="\0".join(names)):
                self.assertEqual(
                    set(campaign.source_manifest(root, "parser")), set(names[:4])
                )
                self.assertEqual(set(campaign.source_manifest(root)), set(names))

    def test_eight_mode_seeds_preserve_inputs_and_cover_profile_boundaries(self):
        for data in [b"", b"int f(a){return a;}", b"a" * 255, b"a" * 512, b"a" * 1023]:
            for count in (5, 7, 11, 32):
                seeds = list(campaign.seed_profiles(data, count, modes=8))
                self.assertEqual(len(seeds), count * 8)
                self.assertEqual(
                    {(sum(seed) % count, (sum(seed) >> 8) & 7) for seed in seeds},
                    {(profile, mode) for profile in range(count) for mode in range(8)},
                )
                self.assertTrue(all(seed.startswith(data) for seed in seeds))

    def test_preprocessor_padding_covers_all_policies_without_changing_source(self):
        for source in [
            b"",
            b"int x=6 //**/ 2;",
            b"a" * 255,
            b"a" * 256,
            b"a" * 511,
            b"a" * 2560,
        ]:
            seeds = list(campaign.seed_preprocessor_policies(source))
            self.assertEqual(len(seeds), 480)
            self.assertEqual({(sum(seed) >> 9) % 5 for seed in seeds}, set(range(5)))
            self.assertEqual(
                {
                    (
                        (sum(seed) >> 9) % 5,
                        bool(sum(seed) & 0x100),
                        sum(seed) & 1,
                        bool(sum(seed) & 2),
                        bool(sum(seed) & 4),
                        bool(sum(seed) & 8),
                        (sum(seed) >> 4) & 3,
                    )
                    for seed in seeds
                },
                {
                    (
                        comment,
                        trigraph,
                        dialect,
                        scope,
                        history,
                        redefine,
                        documentation,
                    )
                    for comment in range(5)
                    for trigraph in (False, True)
                    for dialect in range(2)
                    for scope in (False, True)
                    for history in (False, True)
                    for redefine in (False, True)
                    for documentation in range(3)
                },
            )
            self.assertTrue(
                all(
                    seed.startswith(source + b"\n/* profile ")
                    and seed.endswith(b" */\n")
                    for seed in seeds
                )
            )

    def test_profile_seeds_and_initial_archive_preserve_exact_input_bytes(self):
        source = b"int f(void) { return 0; }\n"
        for count in (5, 7, 11, 32):
            seeds = list(campaign.seed_profiles(source, count))
            self.assertEqual(
                [sum(seed) % count for seed in seeds], list(range(count)) * 2
            )
            self.assertEqual(
                [bool(sum(seed) & 0x100) for seed in seeds],
                [False] * count + [True] * count,
            )
            self.assertTrue(all(seed.startswith(source) for seed in seeds))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            for seed in campaign.seed_profiles(source, 7):
                (corpus / hashlib.sha256(seed).hexdigest()).write_bytes(seed)
            (corpus / "invalid").write_bytes(b"valid\xff")
            manifest = campaign.archive_initial_corpus(corpus, root)
            (corpus / "invalid").write_bytes(b"mutated")
            with tarfile.open(root / "initial-corpus.tar.gz") as archive:
                restored = {
                    member.name: archive.extractfile(member).read()
                    for member in archive.getmembers()
                }
            self.assertEqual(restored["invalid"], b"valid\xff")
            self.assertEqual(
                {
                    name: hashlib.sha256(data).hexdigest()
                    for name, data in restored.items()
                },
                {name: entry["sha256"] for name, entry in manifest.items()},
            )

    def test_mode_padding_covers_boundary_checksums_and_preprocessing(self):
        for source in [b"", b"a" * 255, b"a" * 256, b"a" * 511]:
            for count in (None, 1, 2, 7, 11, 31, 32):
                seeds = list(campaign.seed_profiles(source, count))
                self.assertEqual(len(seeds), 2 * (count or 1))
                self.assertEqual(
                    {
                        (sum(seed) % (count or 1), bool(sum(seed) & 0x100))
                        for seed in seeds
                    },
                    {
                        (profile, mode)
                        for profile in range(count or 1)
                        for mode in (False, True)
                    },
                )
                self.assertTrue(
                    all(
                        seed.startswith(source + b"\n/* profile ")
                        and seed.endswith(b" */\n")
                        for seed in seeds
                    )
                )

    def test_failures_cannot_be_reported_as_clean_campaigns(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = campaign.run_fuzzer(
                [sys.executable, "-c", "print('stat::number_of_executed_units: 17')"],
                root,
                root,
                1,
            )
            clean = dict(result, artifacts={}, source_unchanged=True)
            self.assertTrue(campaign.campaign_passed(clean))
            for change in (
                {"exit_code": 1},
                {"wall_timeout": True},
                {"statistics": {}},
                {"artifacts": {"crash": {}}},
                {"source_unchanged": False},
            ):
                self.assertFalse(campaign.campaign_passed(clean | change), change)
            with patch.object(
                campaign.subprocess,
                "run",
                side_effect=subprocess.TimeoutExpired("fuzzer", 61),
            ):
                result = campaign.run_fuzzer(["fuzzer"], root, root, 1)
            self.assertTrue(result["wall_timeout"])
            self.assertIsNone(result["exit_code"])


if __name__ == "__main__":
    unittest.main()
