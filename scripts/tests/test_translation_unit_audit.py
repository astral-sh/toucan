"""Tests for build-command fidelity and audit failure classification."""

import json
import os
import shlex
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import audit_translation_units as audit


class TranslationUnitAuditTests(unittest.TestCase):
    def test_compiler_profile_prefers_clang_over_its_gnu_compatibility_macros(self):
        self.assertEqual(audit.compiler_profile("#define __GNUC__ 13\n"), "gcc")
        self.assertEqual(
            audit.compiler_profile("#define __GNUC__ 4\n#define __clang__ 1\n"),
            "clang",
        )
        for text in ("", "__clang__", "#define __GNUC_MINOR__ 3\n"):
            with self.assertRaisesRegex(RuntimeError, "supported GNU or Clang"):
                audit.compiler_profile(text)

    def test_cmake_selects_exact_target_and_keeps_quoted_definitions(self):
        with tempfile.TemporaryDirectory(prefix="toucan commands ") as temporary:
            root = Path(temporary)
            source = root / "source file.c"
            argv = [
                "cc",
                '-DHEADER="file name.h"',
                "-I",
                "include dir",
                "-O3",
                "-o",
                "CMakeFiles/static.dir/source.o",
                "-c",
                str(source),
            ]
            database = root / "compile_commands.json"
            database.write_text(
                json.dumps(
                    [
                        {
                            "directory": str(root),
                            "file": str(source),
                            "command": shlex.join(argv),
                        },
                        {
                            "directory": str(root),
                            "file": str(source),
                            "arguments": [
                                part.replace("static.dir", "shared.dir")
                                for part in argv
                            ],
                        },
                    ]
                )
            )
            command, cwd, provenance = audit.compilation(
                {"build": str(root)},
                {"id": "case", "command_kind": "cmake", "cmake_target": "static"},
                source,
            )
            self.assertEqual(command, argv)
            self.assertEqual(provenance, database)
            flags = audit.analysis_flags(command, cwd, source)
            self.assertEqual(
                flags, ['-DHEADER="file name.h"', "-I", "include dir", "-O3"]
            )
            profile = audit.toucan_flags(flags, cwd)
            self.assertEqual(profile["definitions"], [["HEADER", '"file name.h"']])
            self.assertEqual(profile["include_dirs"], [str(root / "include dir")])
            self.assertEqual(profile["unmodeled_compiler_flags"], ["-O3"])

    def test_dependency_parser_matches_native_compiler_escaping(self):
        with tempfile.TemporaryDirectory(prefix="toucan deps # $ ") as temporary:
            root = Path(temporary)
            header = root / "header # $.h"
            source = root / "source $.c"
            header.write_text("typedef int Value;\n")
            source.write_text('#include "header # $.h"\nValue x;\n')
            command = [os.environ.get("CC", "cc"), "-M", "-MT", "audit", str(source)]
            output = subprocess.run(command, check=True, capture_output=True, text=True)
            dependencies = audit.dependency_paths(output.stdout, root)
            self.assertIn(header, dependencies)
            self.assertIn(source, dependencies)
            self.assertTrue(all(path.is_file() for path in dependencies))

    def test_ordered_macro_flags_and_unsupported_include_policy(self):
        profile = audit.toucan_flags(
            ["-DVALUE=1", "-UVALUE", "-D", "VALUE=2", "-DNDEBUG", "-std=c90"],
            Path.cwd(),
        )
        self.assertEqual(
            profile["definitions"],
            [["VALUE", "1"], ["VALUE", None], ["VALUE", "2"], ["NDEBUG", "1"]],
        )
        self.assertEqual(profile["unmodeled_compiler_flags"], [])
        self.assertEqual(profile["language_mode"], "c90")
        for flag in ("-include", "-imacros", "-iquote", "-isystem", "-nostdinc"):
            with self.assertRaises(RuntimeError):
                audit.toucan_flags([flag, "header"], Path.cwd())

    def test_language_modes_preserve_last_option_and_unknown_flags(self):
        for flags, mode, unmodeled in [
            (["-std=c11"], "c11", []),
            (["-std=c11", "-std=gnu11"], "gnu11", []),
            (["-std=c11", "-std=c90"], "c90", []),
            (["-std=c90", "-std=c11"], "c11", []),
            (["-std=c89"], "c90", []),
            (["-std=gnu89"], "gnu90", []),
            (["-std=iso9899:1990"], "c90", []),
            (["-std=c17", "-std=c90"], "c90", ["-std=c17"]),
            (["-std=c90", "-std=c17"], "gnu11", ["-std=c17"]),
        ]:
            profile = audit.toucan_flags(flags, Path.cwd())
            self.assertEqual(profile["language_mode"], mode)
            self.assertEqual(profile["unmodeled_compiler_flags"], unmodeled)
        profile = audit.toucan_flags([], Path.cwd())
        self.assertIn("compiler default not inferred", profile["language_mode_source"])

    def test_modified_header_cannot_reuse_an_archive_pin(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            header = source / "header.h"
            header.write_text("typedef int Value;\n")
            archive = root / "source.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(header, arcname="pin/header.h")
            project = {
                "source": str(source),
                "archive_root": "pin",
                "archive": archive.name,
            }
            audit.verify_source_dependencies(project, audit.hashes([header]), root)
            header.write_text("typedef long Value;\n")
            with self.assertRaisesRegex(RuntimeError, "differs from pinned archive"):
                audit.verify_source_dependencies(project, audit.hashes([header]), root)

    def test_complete_output_is_required_for_parity(self):
        normal = {"result": {"status": "accepted", "retained": False}}
        retained = {"result": {"status": "accepted", "retained": True}}
        self.assertFalse(audit.parity(normal, retained))
        normal["declaration_sha256"] = "a" * 64
        retained["declaration_sha256"] = "b" * 64
        self.assertFalse(audit.parity(normal, retained))
        retained["declaration_sha256"] = "a" * 64
        self.assertTrue(audit.parity(normal, retained))
        for result in [
            {"status": "tool_error", "diagnostic": "output cap"},
            {"status": "rejected", "stage": "analysis", "diagnostic": "unsupported X"},
        ]:
            pair = {"result": result}
            self.assertEqual(audit.parity(pair, pair), result["status"] == "rejected")

    def test_exit_code_cannot_claim_success_after_partial_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "result.json"
            output.write_text('{"status":"accepted","retained":false}')
            args = type("Args", (), {"probe": Path("probe"), "timeout": 1})()
            with patch.object(
                audit,
                "run",
                return_value={"stdout": str(output), "exit_code": 2, "timeout": False},
            ):
                result = audit.probe(
                    {"output": str(root / "partial.unit")}, args, root, root, "normal"
                )
            self.assertEqual(result["result"]["status"], "tool_error")
            self.assertNotIn("declaration_sha256", result)

    def test_success_from_another_profile_cannot_satisfy_the_request(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "result.json"
            output.write_text('{"status":"accepted","compiler":"gcc"}')
            args = type("Args", (), {"probe": Path("probe"), "timeout": 1})()
            with patch.object(
                audit,
                "run",
                return_value={"stdout": str(output), "exit_code": 0, "timeout": False},
            ):
                result = audit.probe(
                    {"compiler": "clang", "output": str(root / "wrong.unit")},
                    args,
                    root,
                    root,
                    "normal",
                )
            self.assertEqual(result["result"]["status"], "tool_error")
            self.assertIn("compiler profile", result["result"]["diagnostic"])
            self.assertNotIn("declaration_sha256", result)


if __name__ == "__main__":
    unittest.main()
