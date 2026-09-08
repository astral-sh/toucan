"""Regression checks for the real-consumer evidence gates."""

import argparse
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_astral_builder import (
    binary_artifact,
    check_lock_versions,
    generated_artifacts,
    prepare_zstd,
)


class LockVersions(unittest.TestCase):
    def check(self, original, tested):
        with tempfile.TemporaryDirectory() as directory:
            before, after = (
                Path(directory) / "before.lock",
                Path(directory) / "after.lock",
            )
            before.write_text(original)
            after.write_text(tested)
            return check_lock_versions(before, after)

    def test_new_version_cannot_replace_an_edge_while_old_version_remains(self):
        before = """[[package]]
name = "consumer"
version = "1"
dependencies = ["dep"]
[[package]]
name = "dep"
version = "1"
"""
        after = (
            before.replace('dependencies = ["dep"]', 'dependencies = ["dep 2"]')
            + """[[package]]
name = "dep"
version = "2"
"""
        )
        with self.assertRaisesRegex(
            RuntimeError, "existing dependency versions changed"
        ):
            self.check(before, after)
        accepted = self.check(
            before,
            after.replace('dependencies = ["dep 2"]', 'dependencies = ["dep 1"]'),
        )
        self.assertEqual(accepted["existing_packages_preserved"], 2)
        self.assertEqual(accepted["added_packages"][0]["version"], "2")

    def test_source_and_checksum_identity_cannot_change(self):
        source = """[[package]]
name = "dep"
version = "1"
source = "registry+https://example.test/index"
checksum = "original"
"""
        with self.assertRaisesRegex(RuntimeError, "existing package identity changed"):
            self.check(
                source, source.replace('checksum = "original"', 'checksum = "changed"')
            )
        with self.assertRaisesRegex(RuntimeError, "existing locked versions"):
            self.check(source, source.replace('version = "1"', 'version = "2"'))

    def test_zstd_path_substitution_and_builder_addition_preserve_old_edges(self):
        before = """[[package]]
name = "zstd-sys"
version = "2.0.16+zstd.1.5.7"
source = "registry+https://example.test/index"
checksum = "original"
dependencies = ["cc"]
[[package]]
name = "cc"
version = "1"
"""
        after = (
            before.replace(
                'source = "registry+https://example.test/index"\nchecksum = "original"\n',
                "",
            ).replace(
                'dependencies = ["cc"]', 'dependencies = ["cc", "toucan_bindgen"]'
            )
            + """[[package]]
name = "toucan_bindgen"
version = "0.0.1"
"""
        )
        accepted = self.check(before, after)
        self.assertEqual(
            accepted["existing_dependency_edge_additions"][0]["added_dependencies"],
            [("toucan_bindgen", "0.0.1", None)],
        )


class PreparedSource(unittest.TestCase):
    def test_reuse_requires_exact_manifest_edits_and_unchanged_bindings(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            original = root / "original"
            (original / "src").mkdir(parents=True)
            (original / "Cargo.toml").write_text(
                '[features]\nstd = []\n[build-dependencies.bindgen]\nversion = "0.72"\n'
            )
            (original / "build.rs").write_text("fn main() {}")
            bindings = original / "src/bindings_zstd.rs"
            bindings.write_text("original checked-in bindings")
            metadata = {
                "packages": [
                    {
                        "name": "zstd-sys",
                        "version": "2.0.16+zstd.1.5.7",
                        "id": "registry+test",
                        "manifest_path": str(original / "Cargo.toml"),
                    }
                ]
            }
            args = argparse.Namespace(
                output=root / "output", zstd_source=None, build_timeout=10
            )
            args.output.mkdir()
            with patch(
                "verify_astral_builder.run",
                return_value={"stdout": json.dumps(metadata)},
            ):
                initial = prepare_zstd(args)
                args.zstd_source = Path(initial["replacement"])
                reused = prepare_zstd(args)
                self.assertTrue(reused["reused_source"])
                (args.zstd_source / "src/bindings_zstd.rs").write_text(
                    "pre-replaced bindings"
                )
                with self.assertRaisesRegex(
                    RuntimeError, "differs from the pinned package"
                ):
                    prepare_zstd(args)


class BinaryArtifacts(unittest.TestCase):
    def test_configured_target_uses_selected_executable_and_ignores_stale_host_path(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "source"
            source.mkdir()
            manifest = source / "Cargo.toml"
            manifest.write_text('[package]\nname="project"\nversion="1.0.0"\n')
            stale = root / "target/debug/tool"
            selected = root / "target/configured-target/debug/tool"
            stale.parent.mkdir(parents=True)
            selected.parent.mkdir(parents=True)
            stale.write_text("stale binary")
            selected.write_text("selected binary")
            artifact = {
                "reason": "compiler-artifact",
                "target": {"name": "tool", "kind": ["bin"]},
                "profile": {"test": False},
                "manifest_path": str(manifest),
                "executable": str(selected),
                "package_id": "project",
            }
            log = root / "cargo.jsonl"
            log.write_text(json.dumps(artifact) + "\n")
            result = binary_artifact(
                log, source, {"binary": "tool", "package": "project"}
            )
            self.assertEqual(result["executable"], str(selected))
            artifact["profile"]["test"] = True
            log.write_text(json.dumps(artifact) + "\n")
            with self.assertRaisesRegex(RuntimeError, "expected one selected"):
                binary_artifact(log, source, {"binary": "tool", "package": "project"})


class GeneratedArtifacts(unittest.TestCase):
    def test_selected_out_dir_input_is_required(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "zstd-sys"
            (source / "src").mkdir(parents=True)
            (source / "src/lib.rs").write_text(
                'include!(concat!(env!("OUT_DIR"), "/bindings.rs"));'
            )
            (source / "Cargo.toml").write_text("")
            target = root / "target"
            target.mkdir()
            rlib = target / "libzstd_sys-example.rlib"
            rlib.write_bytes(b"")
            dep = target / "zstd_sys-example.d"
            binding = target / "build/zstd-sys-example/out/bindings.rs"
            binding.parent.mkdir(parents=True)
            binding.write_text("// Generated by Toucan for the test target.\n")
            dep.write_text(f"{rlib}: {source / 'src/lib.rs'} {binding}\n")
            log = root / "cargo.jsonl"
            artifact = {
                "reason": "compiler-artifact",
                "target": {"name": "zstd_sys"},
                "manifest_path": str(source / "Cargo.toml"),
                "package_id": "test-source",
                "features": ["std", "bindgen"],
                "profile": {},
                "filenames": [str(rlib)],
            }
            log.write_text(json.dumps(artifact) + "\n")
            result = generated_artifacts(log, source, root / "accepted")
            self.assertEqual(result[0]["original_out_dir_binding"], str(binding))
            with self.assertRaisesRegex(RuntimeError, "binding target differs"):
                generated_artifacts(log, source, root / "wrong-target", "wrong-target")
            dep.write_text(
                f"{rlib}: {source / 'src/lib.rs'} {binding} {source / 'src/bindings_zstd.rs'}\n"
            )
            with self.assertRaisesRegex(
                RuntimeError, "pre-generated binding inputs remain active"
            ):
                generated_artifacts(log, source, root / "bad-premade")
            dep.write_text(f"{rlib}: {source / 'src/lib.rs'}\n")
            with self.assertRaisesRegex(
                RuntimeError, "expected one generated OUT_DIR input"
            ):
                generated_artifacts(log, source, root / "bad-missing")
            artifact["features"] = ["std"]
            log.write_text(json.dumps(artifact) + "\n")
            with self.assertRaisesRegex(RuntimeError, "bindgen enabled"):
                generated_artifacts(log, source, root / "bad-feature")


if __name__ == "__main__":
    unittest.main()
