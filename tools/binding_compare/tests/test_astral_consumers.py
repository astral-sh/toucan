import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "verify_astral_consumers", ROOT / "scripts/verify_astral_consumers.py"
)
CONSUMERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CONSUMERS)

LOCK = """version = 4

[[package]]
name = "wasip2"
version = "1.0.1+wasi-0.2.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "retain-the-original-checksum"

[[package]]
name = "zstd-sys"
version = "2.0.16+zstd.1.5.7"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "registry-package-checksum"
dependencies = ["cc", "pkg-config"]
"""


class LockedConsumerTests(unittest.TestCase):
    def test_path_substitution_preserves_the_entire_dependency_graph(self):
        with tempfile.TemporaryDirectory() as directory:
            upstream = Path(directory) / "upstream.lock"
            generated = Path(directory) / "generated.lock"
            upstream.write_text(LOCK)
            generated.write_text(LOCK)
            CONSUMERS.patch_zstd_lockfile(generated)
            CONSUMERS.check_locks(upstream, generated)
            self.assertIn(
                'checksum = "retain-the-original-checksum"', generated.read_text()
            )
            self.assertNotIn(
                'checksum = "registry-package-checksum"', generated.read_text()
            )
            self.assertEqual(generated.read_text().count("source = "), 1)
            generated.write_text(
                generated.read_text().replace("1.0.1+wasi-0.2.4", "1.0.4+wasi-0.2.12")
            )
            with self.assertRaisesRegex(RuntimeError, "extend beyond"):
                CONSUMERS.check_locks(upstream, generated)

    def test_missing_or_duplicate_package_is_rejected_before_writing(self):
        for contents in [
            LOCK.replace('name = "zstd-sys"', 'name = "another-package"'),
            LOCK + LOCK[LOCK.index('[[package]]\nname = "zstd-sys"') :],
        ]:
            with (
                self.subTest(contents=contents),
                tempfile.TemporaryDirectory() as directory,
            ):
                lockfile = Path(directory) / "Cargo.lock"
                lockfile.write_text(contents)
                with self.assertRaisesRegex(RuntimeError, "expected one zstd-sys"):
                    CONSUMERS.patch_zstd_lockfile(lockfile)
                self.assertEqual(lockfile.read_text(), contents)


if __name__ == "__main__":
    unittest.main()
