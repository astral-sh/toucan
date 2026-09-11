#!/usr/bin/env python3
"""Record native symbol evidence for the checked inline-ownership fixtures."""

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

from compiler_diagnostics import has_crash_diagnostic


def capture(command, source=None):
    result = subprocess.run(
        command, input=source, text=True, capture_output=True, timeout=30, check=False
    )
    return {
        "command": command,
        "exit_code": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }


def symbol_line(text, llvm):
    for line in text.splitlines():
        if llvm:
            if line.startswith(("define ", "declare ")) and "@f(" in line:
                return line
        elif line.split()[-1:] == ["f"]:
            return line
    return None


def definition_kind(symbol, llvm):
    if llvm:
        if symbol.startswith("declare "):
            return "InlineOnly"
        for marker, kind in [
            (" internal ", "Internal"),
            (" weak_odr ", "MicrosoftExternInline"),
            (" linkonce_odr ", "MicrosoftInline"),
        ]:
            if marker in symbol:
                return kind
        return "External"
    return {"T": "External", "t": "Internal", "U": "InlineOnly"}[symbol.split()[-2]]


def probe(case, compiler, target, mode, output):
    source = case["source"] + "\n__typeof__(f) *address=f;\n"
    command = [compiler, f"-std={mode}", "-O0", "-fno-inline", "-x", "c", "-"]
    if target:
        command += [f"--target={target}", "-S", "-emit-llvm", "-o", "-"]
    else:
        command += ["-c", "-o", str(output)]
    record = capture(command, source)
    if record["exit_code"] not in (0, 1) or has_crash_diagnostic(
        record["stdout"], record["stderr"]
    ):
        raise RuntimeError(f"compiler failure: {record}")
    record.update(name=case["name"], source=source, target=target, mode=mode)
    record["symbol"] = None
    record["kind"] = None
    if record["exit_code"] == 0:
        if target:
            text = record["stdout"]
        else:
            record["nm"] = capture(["nm", str(output)])
            if record["nm"]["exit_code"] != 0:
                raise RuntimeError(record["nm"])
            text = record["nm"]["stdout"]
        symbol = symbol_line(text, bool(target))
        if symbol is None:
            raise RuntimeError(f"missing address-taken function: {record}")
        record["symbol"] = symbol
        record["kind"] = (
            definition_kind(symbol, bool(target)) if case["has_body"] else "Declaration"
        )
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--gcc", default=os.environ.get("TOUCAN_GCC", "gcc"))
    parser.add_argument("--clang", default=os.environ.get("TOUCAN_CLANG", "clang"))
    args = parser.parse_args()
    fixture = (
        Path(__file__).resolve().parents[1]
        / "crates/toucan_semantic/tests/fixtures/inline_ownership.json"
    )
    cases = json.loads(fixture.read_text())
    versions = {
        compiler: capture([compiler, "--version"])
        for compiler in [args.gcc, args.clang]
    }
    if any(record["exit_code"] != 0 for record in versions.values()):
        raise RuntimeError(versions)
    if "clang" in versions[args.gcc]["stdout"].lower():
        raise ValueError("--gcc must identify GNU GCC")
    records = []
    with tempfile.TemporaryDirectory(prefix="toucan-inline-") as directory:
        output = Path(directory) / "probe.o"
        for compiler, target, family in [
            (args.gcc, None, "gcc"),
            (args.clang, "x86_64-unknown-linux-gnu", "clang"),
            (args.clang, "x86_64-pc-windows-msvc", "msvc"),
        ]:
            for mode in [
                "c90",
                "gnu90",
                "c99",
                "gnu99",
                "c11",
                "gnu11",
                "c17",
                "gnu17",
            ]:
                key = f"{family}:{'gnu90' if mode.endswith('90') else 'gnu11'}"
                for case in cases:
                    record = probe(case, compiler, target, mode, output)
                    record["expected"] = case["expected"][key]
                    record["matches"] = (
                        record["exit_code"] in (0, 1)
                        and record["kind"] == record["expected"]
                    )
                    records.append(record)
    document = {
        "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(),
        "versions": versions,
        "observations": len(records),
        "matched": sum(record["matches"] for record in records),
        "records": records,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(document, indent=2) + "\n")
    print(
        f"{document['matched']}/{document['observations']} native observations matched"
    )
    if document["matched"] != document["observations"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
