"""Check that the i686 consumer evidence rejects mixed-architecture C objects."""

import platform
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_zstd_consumer import has_i686_elf_headers


@unittest.skipUnless(
    platform.system() == "Linux"
    and platform.machine() in ("x86_64", "i386", "i686")
    and all(shutil.which(tool) for tool in ("gcc", "ar", "readelf")),
    "requires GCC with an i686 backend on x86 Linux",
)
class I686ArchiveOracle(unittest.TestCase):
    def test_compiled_elf32_objects_pass_and_mixed_archives_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            source = path / "abi.c"
            source.write_text("int abi(void) { return sizeof(void *); }\n")
            objects = {}
            for arch in ("32", "64"):
                obj = path / f"abi-{arch}.o"
                subprocess.run(
                    ["gcc", f"-m{arch}", "-c", str(source), "-o", str(obj)], check=True
                )
                objects[arch] = obj
                headers = subprocess.check_output(
                    ["readelf", "--wide", "--file-header", str(obj)], text=True
                )
                self.assertEqual(has_i686_elf_headers(headers, 1), arch == "32")

            for name, selected, expected in (
                ("i686", ("32",), True),
                ("mixed", ("32", "64"), False),
            ):
                archive = path / f"{name}.a"
                subprocess.run(
                    ["ar", "rcs", str(archive), *(str(objects[arch]) for arch in selected)],
                    check=True,
                )
                members = subprocess.check_output(
                    ["ar", "t", str(archive)], text=True
                ).splitlines()
                headers = subprocess.check_output(
                    ["readelf", "--wide", "--file-header", str(archive)],
                    text=True,
                )
                self.assertEqual(has_i686_elf_headers(headers, len(members)), expected)


if __name__ == "__main__":
    unittest.main()
