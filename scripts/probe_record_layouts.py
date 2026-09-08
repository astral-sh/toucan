#!/usr/bin/env python3
"""Check every complete generated record against independently compiled C."""

from __future__ import annotations

import argparse
import importlib.util
import json
import platform
from pathlib import Path

# Load the sibling harness even when Python's safe-path mode is enabled.
_spec = importlib.util.spec_from_file_location(
    "verify_corpus", Path(__file__).with_name("verify_corpus.py")
)
assert _spec and _spec.loader
_corpus = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_corpus)
digest, execute = _corpus.digest, _corpus.execute
native_target, parse_output = _corpus.native_target, _corpus.parse_output


def record_probes(
    inventory: dict, bindings: Path, header: Path, target: str, oracle: Path | None
):
    c = [
        f"#include {json.dumps(str(header))}",
        "#include <stddef.h>",
        "#include <stdio.h>",
    ]
    if oracle is not None:
        c.append(f"#include {json.dumps(str(oracle))}")
    c.append("int main(void) {")
    rust = [
        "#![allow(dead_code, non_camel_case_types, non_upper_case_globals, non_snake_case)]",
        f"mod b {{ include!({json.dumps(str(bindings))}); }}",
        "fn main() {",
    ]
    records = []
    offsets = 0
    for name, record in sorted(inventory["records"].items()):
        if record["opaque"]:
            continue
        if name == "__builtin_va_list_record":
            c_type = (
                "__typeof__(((__builtin_va_list){0})[0])"
                if target == "x86_64-unknown-linux-gnu"
                else "__builtin_va_list"
            )
        elif record["aliases"]:
            c_type = record["aliases"][0]
        else:
            c_type = f"{record['kind']} {name}"
        rust_type = record["rust_name"]
        c.append(f'printf("size.{name}=%zu\\n", sizeof({c_type}));')
        c.append(f'printf("align.{name}=%zu\\n", _Alignof({c_type}));')
        rust.append(
            f'println!("size.{name}={{}}", core::mem::size_of::<b::{rust_type}>());'
        )
        rust.append(
            f'println!("align.{name}={{}}", core::mem::align_of::<b::{rust_type}>());'
        )
        for field in record["fields"]:
            field_name = field["name"]
            rust_field = "r#" + field["rust_name"].removeprefix("r#")
            c.append(
                f'printf("offset.{name}.{field_name}=%zu\\n", offsetof({c_type}, {field_name}));'
            )
            rust.append(
                f'println!("offset.{name}.{field_name}={{}}", core::mem::offset_of!(b::{rust_type}, {rust_field}));'
            )
            offsets += 1
        records.append({"name": name, "c_type": c_type, **record})
    c.append("return 0; }")
    rust.append("}")
    return "\n".join(c) + "\n", "\n".join(rust) + "\n", records, offsets


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepared", type=Path, required=True)
    parser.add_argument("--inventory-dir", type=Path, required=True)
    parser.add_argument("--bindings-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target", default=native_target())
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--project", action="append", default=[])
    parser.add_argument("--cc", default="cc")
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--timeout", type=int, default=120)
    args = parser.parse_args()
    if args.target != native_target():
        parser.error("record probes execute natively; the target must match the host")
    results = []
    prepared = json.loads(args.prepared.read_text())
    for project in prepared["projects"]:
        name = project["name"]
        if args.project and name not in args.project:
            continue
        directory = (args.output / name).resolve()
        directory.mkdir(parents=True, exist_ok=True)
        inventory_path = args.inventory_dir / f"{name}-toucan-api.json"
        bindings = (args.bindings_dir / name / "bindings.rs").resolve()
        header = Path(project["header"])
        result = {
            "name": name,
            "status": "failed",
            "header_sha256": digest(header),
            "bindings_sha256": digest(bindings),
            "inventory_sha256": digest(inventory_path),
            "commands": [],
        }
        results.append(result)
        try:
            oracle = _corpus.ROOT / "corpus" / "oracles" / f"{name}.c"
            result["oracle_sha256"] = digest(oracle) if oracle.is_file() else None
            c, rust, records, offsets = record_probes(
                json.loads(inventory_path.read_text()),
                bindings,
                header,
                args.target,
                oracle if oracle.is_file() else None,
            )
            result["records"] = records
            result["record_count"] = len(records)
            result["offset_count"] = offsets
            (directory / "records.c").write_text(c)
            (directory / "records.rs").write_text(rust)
            c_command = [
                args.cc,
                "-std=c11",
                str(directory / "records.c"),
                "-o",
                str(directory / "c-probe"),
            ]
            if platform.system() == "Darwin":
                c_command += ["-isysroot", str(args.sysroot)]
            else:
                c_command += [f"--sysroot={args.sysroot}"]
            for include in project["include_dirs"]:
                c_command += ["-I", include]
            execute(c_command, directory, "compile-c", result["commands"], args.timeout)
            execute(
                [
                    args.rustc,
                    "--edition=2024",
                    "-D",
                    "improper_ctypes",
                    str(directory / "records.rs"),
                    "-o",
                    str(directory / "rust-probe"),
                ],
                directory,
                "compile-rust",
                result["commands"],
                args.timeout,
            )
            c_values = parse_output(
                execute(
                    [str(directory / "c-probe")],
                    directory,
                    "run-c",
                    result["commands"],
                    args.timeout,
                )
            )
            rust_values = parse_output(
                execute(
                    [str(directory / "rust-probe")],
                    directory,
                    "run-rust",
                    result["commands"],
                    args.timeout,
                )
            )
            result["differences"] = {
                key: {"c": c_values.get(key), "rust": rust_values.get(key)}
                for key in sorted(c_values.keys() | rust_values.keys())
                if c_values.get(key) != rust_values.get(key)
            }
            result["comparisons"] = len(c_values)
            if not result["differences"]:
                result["status"] = "passed"
        except (OSError, RuntimeError, ValueError) as error:
            result["error"] = str(error)
        print(f"{name}: {result['status']}", flush=True)
        if "error" in result:
            print(result["error"], flush=True)
    output = {
        "schema_version": 1,
        "target": args.target,
        "sysroot": str(args.sysroot),
        "scope": "Every complete record and ordinary field from the parsed Toucan output. Bitfield accessors require separate FFI tests; opaque records have no layout.",
        "projects": results,
        "status": "passed"
        if results and all(item["status"] == "passed" for item in results)
        else "failed",
    }
    (args.output / "evidence.json").write_text(json.dumps(output, indent=2) + "\n")
    return int(output["status"] != "passed")


if __name__ == "__main__":
    raise SystemExit(main())
