"""Consumer evidence must describe the executable and target actually tested."""

import argparse
import json
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_sqlite_consumer as sqlite
import verify_uv_tls as tls


class TlsExecutable(unittest.TestCase):
    def test_matching_caller_hash_cannot_substitute_another_cargo_binary(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "uv-source"
            manifest = source / "crates/uv/Cargo.toml"
            manifest.parent.mkdir(parents=True)
            manifest.write_text('[package]\nname = "uv"\n')
            (source / "Cargo.lock").write_text("version = 4\n")
            archive = root / "uv.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(source, arcname=source.name)
            project = root / "corpus/consumers/astral.json"
            project.parent.mkdir(parents=True)
            project.write_text(
                json.dumps({"uv": {"archive_sha256": tls.digest(archive)}})
            )
            selected, supplied = root / "built-uv", root / "frozen-uv"
            selected.write_bytes(b"the executable from the audited build")
            supplied.write_bytes(b"an executable from another build")
            log = root / "cargo.jsonl"
            log.write_text(
                json.dumps(
                    {
                        "reason": "compiler-artifact",
                        "target": {"name": "uv", "kind": ["bin"]},
                        "manifest_path": str(manifest),
                        "executable": str(selected),
                    }
                )
                + "\n"
            )
            args = argparse.Namespace(
                archive=archive,
                source=source,
                binary=supplied,
                binary_sha256=tls.digest(supplied),
                cargo_log=log,
            )
            # The small archive and its source inventory are really checked.
            # The only substituted input is the project pin for this fixture.
            with patch.object(tls, "ROOT", root):
                with self.assertRaisesRegex(
                    RuntimeError, "differs from the Cargo-selected executable"
                ):
                    tls.audit(args, root)


class SqliteTarget(unittest.TestCase):
    def test_target_is_checked_before_dependencies_or_builds(self):
        host = "x86_64-unknown-linux-gnu"
        for target, rustc_host in (
            ("aarch64-unknown-linux-gnu", host),
            (host, ""),
            (host, host),
        ):
            with (
                self.subTest(target=target, rustc_host=rustc_host),
                tempfile.TemporaryDirectory() as temporary,
            ):
                root = Path(temporary)
                binary = root / "toucan"
                binary.write_bytes(b"unused frontend")
                output = root / "output"
                calls = []

                def run(command, **kwargs):
                    calls.append(command)
                    if command == ["rustc", "--version", "--verbose"]:
                        kwargs["stdout"].write(f"rustc test\nhost: {rustc_host}\n")
                        return subprocess.CompletedProcess(command, 0)
                    raise RuntimeError("reached Cargo")

                argv = [
                    "verify_sqlite_consumer.py",
                    "--toucan",
                    str(binary),
                    "--cache",
                    str(root / "cache"),
                    "--output",
                    str(output),
                    "--target",
                    target,
                ]
                expected = (
                    "reached Cargo" if target == rustc_host else "match the Rust host"
                )
                with (
                    patch.object(sys, "argv", argv),
                    patch.object(sqlite.platform, "platform", return_value="test host"),
                    patch.object(sqlite.subprocess, "run", side_effect=run),
                ):
                    with self.assertRaisesRegex(RuntimeError, expected):
                        sqlite.main()
                evidence = json.loads((output / "evidence.json").read_text())
                self.assertEqual(evidence["status"], "failed")
                self.assertIn(expected, evidence["error"])
                self.assertEqual(len(calls), 2 if target == rustc_host else 1)


if __name__ == "__main__":
    unittest.main()
