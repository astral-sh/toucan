"""Git dependency identity and non-storing authentication regression tests."""

import contextlib
import copy
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(
    0, str(Path(__file__).resolve().parents[2] / "corpus/consumers/zstd-optin")
)
import git_source as git


class GitSource(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.metadata = {"packages": []}
        for name in sorted(git.PACKAGES):
            crate = self.root / "crates" / name
            (crate / "src").mkdir(parents=True)
            (crate / "Cargo.toml").write_text("manifest")
            (crate / "src/lib.rs").write_text("source")
            self.metadata["packages"].append(
                {
                    "name": name,
                    "version": "0.0.1",
                    "source": git.GIT_SOURCE,
                    "id": git.GIT_SOURCE + "::" + name,
                    "manifest_path": str(crate / "Cargo.toml"),
                    "targets": [{"src_path": str(crate / "src/lib.rs")}],
                }
            )
        self.checkout = patch.object(
            git, "verify_checkout", return_value={"root": str(self.root)}
        )
        self.checkout.start()
        self.addCleanup(self.checkout.stop)

    def test_valid_packages_and_all_library_artifacts(self):
        report = git.verify_packages(self.metadata)
        rows = []
        for identity, package in report["packages"].items():
            binary = self.root / (package["name"] + ".rlib")
            binary.write_bytes(b"artifact")
            rows.append(
                {
                    "reason": "compiler-artifact",
                    "package_id": identity,
                    "manifest_path": package["manifest_path"],
                    "features": [],
                    "fresh": False,
                    "target": {
                        "name": package["name"],
                        "kind": ["lib"],
                        "src_path": package["target_sources"][0],
                    },
                    "filenames": [str(binary)],
                }
            )
        log = self.root / "build.jsonl"
        log.write_text("\n".join(map(json.dumps, rows)))
        self.assertEqual(len(git.verify_artifacts(log, report)), 9)
        rows.pop()
        log.write_text("\n".join(map(json.dumps, rows)))
        with self.assertRaisesRegex(RuntimeError, "all frontend"):
            git.verify_artifacts(log, report)

    def test_git_source_id_cannot_hide_an_escaped_manifest(self):
        metadata = copy.deepcopy(self.metadata)
        escaped = self.root / "live" / "Cargo.toml"
        escaped.parent.mkdir()
        escaped.write_text("live manifest")
        metadata["packages"][0]["manifest_path"] = str(escaped)
        with self.assertRaisesRegex(RuntimeError, "manifest escapes"):
            git.verify_packages(metadata)

    def test_wrong_revision_and_duplicate_package_are_rejected(self):
        metadata = copy.deepcopy(self.metadata)
        metadata["packages"][0]["source"] = git.GIT_SOURCE.replace(
            git.GIT_PIN, "0" * 40
        )
        with self.assertRaisesRegex(RuntimeError, "wrong frontend source"):
            git.verify_packages(metadata)
        metadata = copy.deepcopy(self.metadata)
        metadata["packages"].append(metadata["packages"][0])
        with self.assertRaisesRegex(RuntimeError, "unexpected frontend package set"):
            git.verify_packages(metadata)

    def test_artifact_cannot_rebind_to_another_checkout(self):
        report = git.verify_packages(self.metadata)
        identity, package = next(iter(report["packages"].items()))
        other = self.root / "other" / "Cargo.toml"
        other.parent.mkdir()
        other.write_text("manifest")
        row = {
            "reason": "compiler-artifact",
            "package_id": identity,
            "manifest_path": str(other),
            "target": {
                "name": package["name"],
                "src_path": package["target_sources"][0],
            },
        }
        log = self.root / "build.jsonl"
        log.write_text(json.dumps(row))
        with self.assertRaisesRegex(RuntimeError, "different manifest"):
            git.verify_artifacts(log, report)

    def test_actual_checkout_head_is_checked_before_inventory(self):
        self.checkout.stop()
        with (
            patch.object(git.subprocess, "check_output", return_value="0" * 40),
            self.assertRaisesRegex(RuntimeError, "HEAD differs"),
        ):
            git.verify_checkout(self.root)

    def test_historical_mode_still_requires_the_frozen_inventory(self):
        self.checkout.stop()
        (self.root / "Cargo.toml").write_text("workspace")
        (self.root / "Cargo.lock").write_text("lock")
        (self.root / "source-digests.json").write_text(
            json.dumps(git.inventory(self.root))
        )
        with (
            patch.object(git, "HERE", self.root),
            patch.object(
                git.subprocess,
                "check_output",
                side_effect=[git.GIT_PIN, str(self.root)] * 2,
            ),
        ):
            self.assertEqual(git.verify_checkout(self.root)["head"], git.GIT_PIN)
            (self.root / "Cargo.lock").write_text("modified")
            with self.assertRaisesRegex(RuntimeError, "exact source inventory"):
                git.verify_checkout(self.root)

    def test_extra_files_and_symlink_directories_are_visible(self):
        (self.root / "Cargo.toml").write_text("workspace")
        (self.root / "Cargo.lock").write_text("lock")
        before = git.inventory(self.root)
        (self.root / "crates/added.rs").write_text("unreviewed")
        self.assertNotEqual(before, git.inventory(self.root))
        (self.root / "crates/linked").symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(RuntimeError, "symlink"):
            git.inventory(self.root)


class CurrentSource(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["crates/*"]\n'
            '[workspace.dependencies]\nhelper = {path="crates/toucan_helper"}\n'
        )
        (self.root / "Cargo.lock").write_text("# test lock\n")
        self.metadata = {"packages": []}
        for name in ("toucan_bindgen", "toucan_helper"):
            crate = self.root / "crates" / name
            (crate / "src").mkdir(parents=True)
            manifest = crate / "Cargo.toml"
            manifest.write_text(
                f'[package]\nname="{name}"\nversion="0.0.2"\n'
                + (
                    "[dependencies]\nhelper.workspace=true\n"
                    if name == "toucan_bindgen"
                    else ""
                )
            )
            (crate / "src/lib.rs").write_text("// original\n")
            self.metadata["packages"].append(
                {
                    "name": name,
                    "version": "0.0.2",
                    "source": None,
                    "id": f"path+file://{crate}#{name}@0.0.2",
                    "manifest_path": str(manifest),
                    "targets": [{"src_path": str(crate / "src/lib.rs")}],
                }
            )
        for args in (
            ["init", "-q"],
            ["add", "."],
            [
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        ):
            subprocess.run(["git", "-C", str(self.root), *args], check=True)

    def test_modified_current_checkout_is_accepted_and_then_frozen(self):
        source = self.root / "crates/toucan_helper/src/lib.rs"
        source.write_text("// revision under test, including local edits\n")
        snapshot = git.current_checkout(self.root)
        self.assertIn("crates/toucan_helper/src/lib.rs", snapshot["git_status"])
        report = git.verify_packages(self.metadata, snapshot=snapshot)
        self.assertEqual(
            {p["name"] for p in report["packages"].values()},
            {"toucan_bindgen", "toucan_helper"},
        )
        self.assertEqual(
            report["files"]["crates/toucan_helper/src/lib.rs"], git.digest(source)
        )
        source.write_text("// changed after preparation\n")
        with self.assertRaisesRegex(RuntimeError, "source changed during validation"):
            git.verify_source(report)

    def test_container_ownership_exception_is_scoped_to_selected_root(self):
        command = ["git", "-C", str(self.root), "rev-parse", "HEAD"]
        with patch.dict(os.environ, {"GIT_TEST_ASSUME_DIFFERENT_OWNER": "1"}):
            before = subprocess.run(
                command, capture_output=True, text=True, check=False
            )
            self.assertNotEqual(before.returncode, 0)
            self.assertIn("dubious ownership", before.stderr)
            report = git.current_checkout(self.root)
            git.verify_packages(self.metadata, snapshot=report)
            after = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertNotEqual(after.returncode, 0)
            self.assertIn("dubious ownership", after.stderr)
        with self.assertRaisesRegex(RuntimeError, "not a Git checkout root"):
            git.current_checkout(self.root / "crates")

    def test_lock_edits_after_snapshot_and_foreign_sources_are_rejected(self):
        snapshot = git.current_checkout(self.root)
        self.metadata["packages"][0]["source"] = git.GIT_SOURCE
        with self.assertRaisesRegex(RuntimeError, "wrong frontend source"):
            git.verify_packages(self.metadata, snapshot=snapshot)
        (self.root / "Cargo.lock").write_text("# changed lock\n")
        with self.assertRaisesRegex(RuntimeError, "source changed during validation"):
            git.verify_source(snapshot)

    def test_source_declared_path_escape_and_metadata_target_escape_fail(self):
        escaped = self.root / "outside"
        escaped.mkdir()
        (escaped / "Cargo.toml").write_text('[package]\nname="toucan_helper"\n')
        workspace = self.root / "Cargo.toml"
        workspace.write_text(
            workspace.read_text().replace("crates/toucan_helper", "outside")
        )
        with self.assertRaisesRegex(RuntimeError, "dependency escapes"):
            git.verify_packages(self.metadata, snapshot=git.current_checkout(self.root))
        workspace.write_text(
            workspace.read_text().replace(
                'path="outside"', 'path="crates/toucan_helper"'
            )
        )
        self.metadata["packages"][0]["targets"][0]["src_path"] = str(
            escaped / "Cargo.toml"
        )
        with self.assertRaisesRegex(RuntimeError, "escaped source target"):
            git.verify_packages(self.metadata, snapshot=git.current_checkout(self.root))

    def test_missing_reachable_package_fails(self):
        self.metadata["packages"].pop()
        with self.assertRaisesRegex(RuntimeError, "unexpected frontend package set"):
            git.verify_packages(self.metadata, snapshot=git.current_checkout(self.root))


class Credentials(unittest.TestCase):
    def answer(self, operation, request):
        output = io.StringIO()
        with (
            patch.dict(
                os.environ,
                {"TOUCAN_GIT_TOKEN": "synthetic-test-credential"},
                clear=True,
            ),
            contextlib.redirect_stdout(output),
        ):
            git.credential_request(operation, request)
        return output.getvalue()

    def test_smoke_refuses_to_pass_fetch_credentials_to_builds(self):
        completed = subprocess.run(
            [
                sys.executable,
                "-B",
                str(git.HERE / "run_smoke.py"),
                "--work-dir",
                "/unused",
                "--target-dir",
                "/unused",
            ],
            env=os.environ | {"TOUCAN_GIT_TOKEN": "synthetic-test-credential"},
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("without the Git fetch token", completed.stderr)
        self.assertNotIn(
            "synthetic-test-credential", completed.stderr + completed.stdout
        )

    def test_only_exact_https_repository_get_receives_credentials(self):
        allowed = "protocol=https\nhost=github.com\npath=astral-sh/toucan\n"
        self.assertIn("password=synthetic-test-credential", self.answer("get", allowed))
        for request in [
            allowed.replace("https", "http"),
            allowed.replace("github.com", "other.example"),
            allowed.replace("toucan", "toucan-other"),
        ]:
            self.assertEqual(self.answer("get", request), "quit=true\n")
        self.assertEqual(self.answer("store", allowed), "")
        self.assertEqual(self.answer("erase", allowed), "")


if __name__ == "__main__":
    unittest.main()
