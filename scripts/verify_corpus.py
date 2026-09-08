#!/usr/bin/env python3
"""Compare generated bindings with independent C probes and execute native FFI calls."""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import platform
import re
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PROBES = json.loads((ROOT / "corpus" / "probes.json").read_text())
INTEGER_CONSTANT = re.compile(
    r"^pub const (\w+): (?:::core::primitive::)?([iu])(8|16|32|64|128) =", re.MULTILINE
)
STRING_CONSTANT = re.compile(
    r"^pub const (\w+): &\[(?:::core::primitive::)?u8\] =", re.MULTILINE
)


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def native_target() -> str:
    architecture = {
        "x86_64": "x86_64",
        "AMD64": "x86_64",
        "aarch64": "aarch64",
        "arm64": "aarch64",
    }.get(platform.machine())
    operating_system = {"Linux": "unknown-linux-gnu", "Darwin": "apple-darwin"}.get(
        platform.system()
    )
    if architecture is None or operating_system is None:
        raise RuntimeError(
            "the native corpus harness currently supports Linux and macOS x86_64/AArch64"
        )
    return f"{architecture}-{operating_system}"


def execute(
    command: list[str], directory: Path, name: str, commands: list[dict], timeout: int
) -> str:
    stdout = directory / f"{name}.stdout"
    stderr = directory / f"{name}.stderr"
    entry = {
        "command": command,
        "cwd": str(ROOT),
        "stdout": str(stdout),
        "stderr": str(stderr),
    }
    commands.append(entry)
    started = time.monotonic()
    try:
        with stdout.open("wb") as out, stderr.open("wb") as err:
            process = subprocess.run(
                command, cwd=ROOT, stdout=out, stderr=err, timeout=timeout, check=False
            )
        entry["exit_code"] = process.returncode
        if process.returncode:
            message = "\n".join(stderr.read_text(errors="replace").splitlines()[-50:])
            raise RuntimeError(f"{name} failed ({process.returncode}):\n{message}")
        return stdout.read_text()
    finally:
        entry["seconds"] = time.monotonic() - started


def generate_probes(name: str, header: Path, source: str) -> tuple[str, str, dict]:
    """Generate independent expressions in C and Rust, sharing only their names."""
    c = [
        f"#include {json.dumps(str(header))}",
        "#include <stddef.h>",
        "#include <stdio.h>",
        "static void print_unsigned(unsigned __int128 value) {",
        "  char digits[40]; unsigned n = 0; do { digits[n++] = '0' + value % 10; value /= 10; } while (value);",
        "  while(n) putchar(digits[--n]);",
        "}",
        "static void print_signed(__int128 value) {",
        "  if (value < 0) { putchar('-'); print_unsigned((unsigned __int128)(-(value + 1)) + 1); }",
        "  else print_unsigned((unsigned __int128)value);",
        "}",
        "int main(void) {",
    ]
    rust = [
        "#![allow(dead_code, non_camel_case_types, non_upper_case_globals, non_snake_case)]",
        'mod b { include!("bindings.rs"); }',
        "fn main() {",
    ]
    integers = INTEGER_CONSTANT.findall(source)
    strings = STRING_CONSTANT.findall(source)
    constants = set(re.findall(r"^pub const (\w+):", source, re.MULTILINE))
    covered = {constant for constant, _, _ in integers} | set(strings)
    if not integers or constants != covered:
        raise RuntimeError(
            f"constant probe coverage is incomplete: {len(integers)} integers, unrecognized declarations {sorted(constants - covered)}"
        )
    for constant, sign, bits in integers:
        printer = "print_signed" if sign == "i" else "print_unsigned"
        cast = "__int128" if sign == "i" else "unsigned __int128"
        c.append(
            f"printf(\"constant.{constant}=\"); {printer}(({cast})({constant})); putchar('\\n');"
        )
        rust.append(f'println!("constant.{constant}={{}}", b::{constant});')
        # C sizeof and _Generic independently validate the inferred constant type.
        c.append(f'printf("constant_size.{constant}=%zu\\n", sizeof({constant}));')
        rust.append(
            f'println!("constant_size.{constant}={{}}", core::mem::size_of_val(&b::{constant}));'
        )
        c.append(
            f'printf("constant_signed.{constant}=%d\\n", _Generic(({constant}), unsigned char: 0, unsigned short: 0, unsigned int: 0, unsigned long: 0, unsigned long long: 0, unsigned __int128: 0, default: 1));'
        )
        rust.append(f'println!("constant_signed.{constant}={int(sign == "i")}");')
    for constant in strings:
        c.append(
            f'printf("string.{constant}="); for (size_t i = 0; i < sizeof({constant}); i++) printf("%02x", (unsigned char)({constant})[i]); putchar(\'\\n\');'
        )
        rust.append(
            f'print!("string.{constant}="); for byte in b::{constant} {{ print!("{{byte:02x}}"); }} println!();'
        )
    offset_count = 0
    for record in PROBES[name]:
        c_type, rust_type = record["c"], record["rust"]
        c.append(f'printf("size.{c_type}=%zu\\n", sizeof({c_type}));')
        c.append(f'printf("align.{c_type}=%zu\\n", _Alignof({c_type}));')
        rust.append(
            f'println!("size.{c_type}={{}}", core::mem::size_of::<b::{rust_type}>());'
        )
        rust.append(
            f'println!("align.{c_type}={{}}", core::mem::align_of::<b::{rust_type}>());'
        )
        for field in record["fields"]:
            c.append(
                f'printf("offset.{c_type}.{field}=%zu\\n", offsetof({c_type}, {field}));'
            )
            rust.append(
                f'println!("offset.{c_type}.{field}={{}}", core::mem::offset_of!(b::{rust_type}, {field}));'
            )
            offset_count += 1
    c.append("return 0; }")
    rust.extend(
        ["ffi_test();", "}", (ROOT / "corpus" / "ffi" / f"{name}.rs").read_text()]
    )
    coverage = {
        "integer_constants": len(integers),
        "string_constants": len(strings),
        "records": len(PROBES[name]),
        "field_offsets": offset_count,
        "ffi_fixture": str(ROOT / "corpus" / "ffi" / f"{name}.rs"),
    }
    return "\n".join(c) + "\n", "\n".join(rust) + "\n", coverage


def parse_output(text: str) -> dict[str, str]:
    result = {}
    for line in text.splitlines():
        key, value = line.split("=", 1)
        if key in result:
            raise RuntimeError(f"duplicate probe key: {key}")
        result[key] = value
    return result


def verify(project: dict, args: argparse.Namespace) -> dict:
    name = project["name"]
    directory = args.output / name
    directory.mkdir(parents=True, exist_ok=True)
    commands = []
    result = {
        "name": name,
        "version": project["version"],
        "commit": project["commit"],
        "archive_sha256": project["sha256"],
        "library_sha256": project["library_sha256"],
        "target": args.target,
        "toucan_sha256": args.toucan_sha256,
        "status": "failed",
        "commands": commands,
    }
    try:
        if digest(args.toucan) != args.toucan_sha256:
            raise RuntimeError("the frontend executable changed during verification")
        archive = args.cache / project["archive"]
        if digest(archive) != project["sha256"]:
            raise RuntimeError(f"archive checksum changed after preparation: {archive}")
        for library, checksum in project["library_sha256"].items():
            if digest(Path(library)) != checksum:
                raise RuntimeError(
                    f"library checksum changed after preparation: {library}"
                )
        header = Path(project["header"])
        result["header_sha256"] = digest(header)
        bindings = directory / "bindings.rs"
        report = directory / "bindings-report.json"
        command = [
            str(args.toucan),
            "bindgen",
            str(header),
            "--target",
            args.target,
            "--sysroot",
            str(args.sysroot),
            "--output",
            str(bindings),
            "--report",
            str(report),
        ]
        for include in [*project["include_dirs"], *args.include_dir]:
            command.extend(["-I", include])
        for pattern in project["allowlist"]:
            command.extend(["--allowlist", pattern])
        execute(command, directory, "bindgen", commands, args.timeout)
        if digest(args.toucan) != args.toucan_sha256:
            raise RuntimeError(
                "the frontend executable changed while generating bindings"
            )
        metadata = json.loads(report.read_text())
        result["bindings_report"] = metadata
        result["dependency_sha256"] = {
            path: digest(Path(path))
            for path in metadata["dependencies"]
            if Path(path).is_file()
        }
        result["bindings_sha256"] = digest(bindings)
        result["bindings_bytes"] = bindings.stat().st_size
        c, rust, coverage = generate_probes(name, header, bindings.read_text())
        result["coverage"] = coverage
        (directory / "probe.c").write_text(c)
        (directory / "probe.rs").write_text(rust)
        c_executable = directory / "c-probe"
        rust_executable = directory / "rust-probe"
        c_command = [
            args.cc,
            "-std=c11",
            str(directory / "probe.c"),
            "-o",
            str(c_executable),
        ]
        if platform.system() == "Darwin":
            c_command.extend(["-isysroot", str(args.sysroot)])
        else:
            c_command.append(f"--sysroot={args.sysroot}")
        for include in project["include_dirs"]:
            c_command.extend(["-I", include])
        execute(c_command, directory, "compile-c", commands, args.timeout)
        rust_command = [
            args.rustc,
            "--edition=2024",
            "-D",
            "improper_ctypes",
            "-D",
            "improper_ctypes_definitions",
            str(directory / "probe.rs"),
            "-o",
            str(rust_executable),
        ]
        helper = ROOT / "corpus" / "ffi" / f"{name}.c"
        if helper.is_file():
            helper_object = directory / "ffi-helper.o"
            helper_command = [
                args.cc,
                "-std=c11",
                "-c",
                str(helper),
                "-o",
                str(helper_object),
            ]
            if platform.system() == "Darwin":
                helper_command.extend(["-isysroot", str(args.sysroot)])
            else:
                helper_command.append(f"--sysroot={args.sysroot}")
            for include in project["include_dirs"]:
                helper_command.extend(["-I", include])
            execute(helper_command, directory, "compile-c-ffi", commands, args.timeout)
            rust_command.extend(["-C", f"link-arg={helper_object}"])
        for library in project["libraries"]:
            archive = Path(library)
            if not archive.name.startswith("lib") or archive.suffix != ".a":
                raise RuntimeError(f"unsupported native archive name: {archive}")
            # Let rustc place native archives before their runtime dependencies.
            # Raw link-args go after libc and lose symbols under --as-needed
            # (notably AArch64's stack protector runtime).
            rust_command.extend(
                [
                    "-L",
                    f"native={archive.parent}",
                    "-l",
                    f"static={archive.stem[3:]}",
                ]
            )
        if platform.system() == "Linux":
            for library in ["m", "dl", "pthread"]:
                rust_command.extend(["-l", library])
        execute(rust_command, directory, "compile-rust", commands, args.timeout)
        c_output = execute(
            [str(c_executable)], directory, "run-c", commands, args.timeout
        )
        rust_output = execute(
            [str(rust_executable)], directory, "run-rust-ffi", commands, args.timeout
        )
        expected, actual = parse_output(c_output), parse_output(rust_output)
        differences = {
            key: {"c": expected.get(key), "rust": actual.get(key)}
            for key in sorted(expected.keys() | actual.keys())
            if expected.get(key) != actual.get(key)
        }
        result["comparisons"] = len(expected)
        result["differences"] = differences
        if differences:
            raise RuntimeError(
                f"{len(differences)} C/Rust probe mismatches: {json.dumps(differences)[:2000]}"
            )
        if metadata.get("skipped_declarations"):
            result["scope_note"] = (
                "see bindings_report.skipped_declarations for selected declarations not emitted"
            )
        result["status"] = "passed"
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        result["error"] = str(error)
    (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(
        f"{name}: {result['status']}"
        + (f"\n{result['error']}" if "error" in result else ""),
        flush=True,
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=ROOT / "corpus" / "cache")
    parser.add_argument("--output", type=Path, default=ROOT / "corpus" / "results")
    parser.add_argument(
        "--toucan", type=Path, default=ROOT / "target" / "debug" / "toucan"
    )
    parser.add_argument("--sysroot", type=Path)
    parser.add_argument("--target", default=native_target())
    parser.add_argument("--cc", default=os.environ.get("CC", "cc"))
    parser.add_argument("--rustc", default=os.environ.get("RUSTC", "rustc"))
    parser.add_argument("--include-dir", action="append", default=[])
    parser.add_argument("--project", action="append", choices=list(PROBES))
    parser.add_argument("--timeout", type=int, default=300)
    args = parser.parse_args()
    if args.target != native_target():
        parser.error(
            "native FFI execution requires the host target; cross-target checks belong to toucan_target"
        )
    if args.sysroot is None:
        if platform.system() == "Darwin":
            parser.error("macOS requires --sysroot with the SDK path")
        args.sysroot = Path("/")
    args.sysroot = args.sysroot.resolve()
    args.toucan = args.toucan.resolve()
    args.toucan_sha256 = digest(args.toucan)
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    # Compiler resource headers are test inputs, explicitly supplied to the frontend.
    resource_headers = subprocess.check_output(
        [args.cc, "-print-file-name=include"], text=True
    ).strip()
    if Path(resource_headers).is_dir():
        args.include_dir.append(str(Path(resource_headers).resolve()))
    prepared = json.loads((args.cache / "prepared.json").read_text())
    projects = prepared["projects"]
    if args.project:
        projects = [project for project in projects if project["name"] in args.project]
    expected_names = set(args.project or PROBES)
    missing = sorted(expected_names - {project["name"] for project in projects})
    evidence = {
        "schema_version": 1,
        "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "host": platform.platform(),
        "target": args.target,
        "sysroot": str(args.sysroot),
        "cc": subprocess.check_output([args.cc, "--version"], text=True).splitlines()[
            0
        ],
        "rustc": subprocess.check_output([args.rustc, "--version"], text=True).strip(),
        "toucan_sha256": args.toucan_sha256,
        "missing_projects": missing,
        "projects": [verify(project, args) for project in projects],
    }
    evidence["status"] = (
        "passed"
        if not missing
        and all(project["status"] == "passed" for project in evidence["projects"])
        else "failed"
    )
    output = args.output / "evidence.json"
    output.write_text(json.dumps(evidence, indent=2) + "\n")
    print(output)
    return int(evidence["status"] != "passed")


if __name__ == "__main__":
    raise SystemExit(main())
