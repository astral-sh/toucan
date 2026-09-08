"""Reproducer preservation and failure classification for sustained fuzzing."""

import hashlib
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
    def test_four_mode_seeds_preserve_inputs_and_cover_profile_boundaries(self):
        for data in [b"", b"int f(a){return a;}", b"a" * 255, b"a" * 512, b"a" * 1023]:
            for count in (5, 7, 11, 32):
                seeds = list(campaign.seed_profiles(data, count, modes=4))
                self.assertEqual(len(seeds), count * 4)
                self.assertEqual(
                    {(sum(seed) % count, (sum(seed) >> 8) & 3) for seed in seeds},
                    {(profile, mode) for profile in range(count) for mode in range(4)},
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
            self.assertEqual(len(seeds), 40)
            self.assertEqual({(sum(seed) >> 9) % 5 for seed in seeds}, set(range(5)))
            self.assertEqual(
                {
                    (
                        (sum(seed) >> 9) % 5,
                        bool(sum(seed) & 0x100),
                        sum(seed) & 1,
                        bool(sum(seed) & 2),
                    )
                    for seed in seeds
                },
                {
                    (comment, trigraph, dialect, scope)
                    for comment in range(5)
                    for trigraph in (False, True)
                    for dialect in range(2)
                    for scope in (False, True)
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
