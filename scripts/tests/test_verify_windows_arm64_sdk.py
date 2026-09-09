"""SDK evidence must retain failures and enforce the selected native inputs."""

import json
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_windows_arm64_sdk as sdk


class SdkInputs(unittest.TestCase):
    def test_selected_version_and_include_order(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            version = "10.0.26100.0"
            selected = root / "Include" / version
            directories = [root / "VC include", *(selected / part for part in ("shared", "um", "ucrt"))]
            for directory in directories:
                directory.mkdir(parents=True)
            environment = {"WindowsSdkDir": str(root), "WindowsSDKVersion": version + "\\", "INCLUDE": ";".join(f'"{directory}"' for directory in directories) + ";"}
            expected = ([directory.resolve() for directory in directories], selected.resolve())
            self.assertEqual(sdk.sdk_configuration(environment), expected)
            # vcvarsall can select the UCRT separately from the UM/shared SDK.
            older_ucrt = root / "Include" / "10.0.22000.0" / "ucrt"
            older_ucrt.mkdir(parents=True)
            environment.update(UCRTVersion="10.0.22000.0", UniversalCRTSdkDir=str(root))
            environment["INCLUDE"] = ";".join(map(str, [*directories[:-1], older_ucrt]))
            self.assertEqual(sdk.sdk_configuration(environment)[0][-1], older_ucrt.resolve())
            environment["INCLUDE"] = ";".join(map(str, directories[:-1]))
            with self.assertRaisesRegex(RuntimeError, "selected SDK ucrt"):
                sdk.sdk_configuration(environment)

    def test_changed_header_is_not_silently_recaptured(self):
        with tempfile.TemporaryDirectory() as temporary:
            header = Path(temporary) / "header.h"
            header.write_text("typedef int T;\n")
            inputs = {}
            sdk.record_inputs([header], inputs)
            header.write_text("typedef short T;\n")
            with self.assertRaisesRegex(RuntimeError, "SDK input changed"):
                sdk.record_inputs([header], inputs)

    def test_coff_and_pe_machine_fields(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "probe"
            for data in (b"\x64\xaa" + bytes(18), b"\0\0\xff\xff\x02\0\x64\xaa" + bytes(48), b"MZ" + bytes(58) + (64).to_bytes(4, "little") + b"PE\0\0\x64\xaa"):
                path.write_bytes(data)
                self.assertEqual(sdk.machine(path), 0xAA64)
            path.write_bytes(b"\x64\x86" + bytes(18))
            self.assertNotEqual(sdk.machine(path), 0xAA64)
            path.write_bytes(b"MZ" + bytes(62) + b"bad PE")
            with self.assertRaisesRegex(RuntimeError, "PE signature"):
                sdk.machine(path)


class FailureRecording(unittest.TestCase):
    def test_failed_command_and_missing_phase_cannot_pass(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runner = sdk.Runner(root, {"commands": {}})
            failed = runner.run([sys.executable, "-c", "import sys; print('native diagnostic', file=sys.stderr); sys.exit(7)"], "compiler")
            self.assertEqual(failed["exit_code"], 7)
            self.assertEqual(failed["status"], "failed")
            self.assertIn("native diagnostic", (root / "compiler.stderr").read_text())
            stages = {phase: {"status": "passed"} for phase in sdk.REQUIRED_STAGES}
            self.assertTrue(sdk.completed(stages))
            stages["msvc"] = failed
            self.assertFalse(sdk.completed(stages))
            del stages["msvc"]
            self.assertFalse(sdk.completed(stages))

    def test_expired_deadline_does_not_start_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            marker = root / "started"
            runner = sdk.Runner(root, {"commands": {}})
            runner.deadline = time.monotonic() - 1
            entry = runner.run([sys.executable, "-c", f"from pathlib import Path; Path({str(marker)!r}).touch()"], "late")
            self.assertEqual(entry["status"], "failed")
            self.assertIsNone(entry["exit_code"])
            self.assertFalse(marker.exists())

    def test_non_windows_attempt_records_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "output"
            arguments = ["verify_windows_arm64_sdk.py", "--toucan", "unused", "--output", str(output)]
            with mock.patch.object(sys, "argv", arguments), mock.patch.object(sys, "platform", "linux"):
                self.assertEqual(sdk.main(), 1)
            evidence = json.loads((output / "evidence.json").read_text())
            self.assertEqual(evidence["status"], "failed")
            self.assertFalse(evidence["native_execution"])
            self.assertIn("native Windows ARM64", evidence["error"])


if __name__ == "__main__":
    unittest.main()
