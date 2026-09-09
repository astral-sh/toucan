"""Application-driver source selection without fetching or compiling dependencies."""

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_astral_optin as optin


class StopAtPreparation(Exception):
    pass


class SourceSelection(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.arguments = [
            "verify_astral_optin.py",
            "--project",
            "uv",
            "--cache",
            str(self.root / "cache"),
            "--output",
            str(self.root / "output"),
            "--prepare-only",
            "--rust-toolchain",
            "ohm",
        ]
        self.environment = patch.dict(os.environ, {}, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def prepare_command(self, arguments):
        commands = []

        def run(command, **kwargs):
            commands.append(command)
            if str(optin.INTEGRATION / "prepare.py") in command:
                raise StopAtPreparation
            self.assertIn("--version", command)
            kwargs["stdout"].write(f"host: {optin.TARGET}\n")
            return subprocess.CompletedProcess(command, 0)

        with (
            patch.object(sys, "argv", self.arguments + arguments),
            patch.object(optin, "download"),
            patch.object(optin.platform, "platform", return_value="test-platform"),
            patch.object(optin, "load_report", return_value={"root": "/fetched"}),
            patch.object(optin.subprocess, "run", side_effect=run),
            self.assertRaises(StopAtPreparation),
        ):
            optin.main()
        self.assertEqual(len(commands), 3)
        return commands[-1]

    def test_local_mode_defaults_to_driver_checkout(self):
        command = self.prepare_command([])
        self.assertEqual(command[command.index("--toucan-source") + 1], str(optin.ROOT))
        self.assertNotIn("--git-source-report", command)

    def test_local_mode_passes_explicit_immutable_source(self):
        source = self.root / "intermediate/../frozen"
        command = self.prepare_command(["--toucan-source", str(source)])
        self.assertEqual(
            command[command.index("--toucan-source") + 1], str(source.resolve())
        )
        self.assertNotEqual(source.resolve(), optin.ROOT)
        self.assertNotIn("--git-source-report", command)

    def test_git_mode_passes_only_the_verified_report(self):
        report = self.root / "git-source.json"
        command = self.prepare_command(
            ["--frontend-mode", "git", "--git-source-report", str(report)]
        )
        self.assertEqual(command[command.index("--git-source-report") + 1], str(report))
        self.assertNotIn("--toucan-source", command)

    def test_conflicting_modes_fail_before_any_external_command(self):
        cases = [
            (["--frontend-mode", "git"], "Git mode requires --git-source-report"),
            (["--git-source-report", "/report"], "local mode does not use it"),
            (
                [
                    "--frontend-mode",
                    "git",
                    "--git-source-report",
                    "/report",
                    "--toucan-source",
                    str(optin.ROOT),
                ],
                "Git mode does not accept --toucan-source",
            ),
        ]
        for arguments, message in cases:
            with (
                self.subTest(arguments=arguments),
                patch.object(sys, "argv", self.arguments + arguments),
                patch.object(optin, "load_report") as load,
                patch.object(optin.subprocess, "run") as run,
                self.assertRaisesRegex(RuntimeError, message),
            ):
                optin.main()
            load.assert_not_called()
            run.assert_not_called()
            self.assertFalse((self.root / "output").exists())


if __name__ == "__main__":
    unittest.main()
