"""The ARMv7 acceptance gate must reject wrong ABIs and record failed runs."""

import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_armv7_native as armv7

HEADER = """Class: ELF32
Machine: ARM
Flags: 0x5000000, Version5 EABI
"""
ATTRIBUTES = """Tag_CPU_arch: v7
Tag_ABI_VFP_args: VFP registers
"""


class Armv7Acceptance(unittest.TestCase):
    def test_elf32_armv7_vfp_object_and_hard_float_executable(self):
        # LLVM emits hard-float VFP attributes on relocatable objects, but
        # the ELF header's hard-float flag appears on linked executables.
        self.assertIsNone(armv7.arm_elf_error(HEADER, ATTRIBUTES, executable=False))
        self.assertEqual(
            armv7.arm_elf_error(HEADER, ATTRIBUTES, executable=True),
            "missing hard-float executable ABI",
        )
        executable = HEADER.replace("Version5 EABI", "Version5 EABI, hard-float ABI")
        self.assertIsNone(armv7.arm_elf_error(executable, ATTRIBUTES, executable=True))

    def test_soft_float_or_wrong_cpu_cannot_pass(self):
        self.assertEqual(
            armv7.arm_elf_error(
                HEADER,
                ATTRIBUTES.replace("VFP registers", "base standard"),
                executable=False,
            ),
            "missing VFP register arguments",
        )
        self.assertEqual(
            armv7.arm_elf_error(
                HEADER.replace("Machine: ARM", "Machine: Intel 80386"),
                ATTRIBUTES,
                executable=False,
            ),
            "missing ARM machine",
        )
        self.assertEqual(
            armv7.arm_elf_error(
                HEADER, ATTRIBUTES.replace("v7", "v6"), executable=False
            ),
            "missing ARMv7",
        )

    def test_wrong_host_records_failure_without_qemu_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            toucan = root / "toucan"
            toucan.write_bytes(b"dummy")
            output = root / "evidence"
            with (
                mock.patch.object(armv7.platform, "system", return_value="Linux"),
                mock.patch.object(armv7.platform, "machine", return_value="aarch64"),
                self.assertRaisesRegex(RuntimeError, "x86_64 Linux runner"),
            ):
                armv7.verify(output, toucan, "clang", preflight=False)
            evidence = json.loads((output / "evidence.json").read_text())
            self.assertEqual(evidence["status"], "failed")
            self.assertFalse(evidence["qemu_execution"])
            self.assertFalse(evidence["native_hardware_execution"])
            self.assertEqual(evidence["runs"], [])


if __name__ == "__main__":
    unittest.main()
