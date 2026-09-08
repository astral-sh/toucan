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
    def test_profile_seeds_and_initial_archive_preserve_exact_input_bytes(self):
        source = b"int f(void) { return 0; }\n"
        for count in (5, 7, 32):
            seeds = list(campaign.seed_profiles(source, count))
            self.assertEqual([sum(seed) % count for seed in seeds], list(range(count)))
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
