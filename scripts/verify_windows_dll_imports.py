#!/usr/bin/env python3
"""Verify generated DLL imports with two COFF libraries and optional Windows execution."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

TARGET = "x86_64-pc-windows-msvc"
TYPES = "typedef int (*Callback)(int);\n"
DATA = """__declspec(dllimport) int a_value;
__declspec(dllimport) int b_value;
__declspec(dllimport) int a_special;
__declspec(dllimport) int unselected;
extern int ordinary;
"""
FUNCTIONS = """__declspec(dllimport) int a_function(int);
__declspec(dllimport) int b_function(int);
"""
ADDRESSES = """__declspec(dllimport) int *a_address_data(void);
__declspec(dllimport) Callback a_address_function(void);
"""
DLL_A = (
    TYPES
    + """__declspec(dllexport) int a_value=11;
__declspec(dllexport) int a_function(int x){return a_value+x;}
__declspec(dllexport) int *a_address_data(void){return &a_value;}
__declspec(dllexport) Callback a_address_function(void){return &a_function;}
"""
)
DLL_B = """__declspec(dllexport) int b_value=7;
__declspec(dllexport) int a_special=23;
__declspec(dllexport) int b_function(int x){return b_value*x;}
"""
C_PROBE = """#include "api.h"
int c_probe(void) {
    if(a_value!=11 || b_value!=7 || a_special!=23 || ordinary!=5) return 1;
    if(a_function(2)!=13 || b_function(4)!=28) return 2;
    if(a_address_data()!=&a_value || a_address_function()!=&a_function) return 3;
    a_value=17;
    if(a_function(2)!=19) return 4;
    a_value=11;
    return 0;
}
"""
RUST_PROBE = """include!("bindings.rs");
extern "C" {fn c_probe()->i32;}
#[no_mangle]
pub unsafe extern "C" fn entry()->i32 {
    if c_probe()!=0 {return 10;}
    if a_value!=11 || b_value!=7 || a_special!=23 || ordinary!=5 {return 11;}
    if a_function(2)!=13 || b_function(4)!=28 {return 12;}
    if a_address_data()!=core::ptr::addr_of_mut!(a_value) {return 13;}
    match a_address_function() {
        Some(callback) => if callback as usize!=a_function as usize {return 14;},
        None => return 14,
    }
    a_value=17;
    if a_function(2)!=19 {return 15;}
    a_value=11;
    0
}
"""


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--clang", default=os.environ.get("TOUCAN_CLANG", "clang"))
    parser.add_argument("--rustc", action="append", default=[])
    parser.add_argument("--linker", type=Path)
    parser.add_argument("--readobj")
    parser.add_argument(
        "--native", action="store_true", help="Execute the Rust and C probes on Windows"
    )
    args = parser.parse_args()
    if args.native and sys.platform != "win32":
        parser.error(
            "--native requires Windows; cross linking does not prove execution"
        )
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    evidence = {
        "target": TARGET,
        "native_execution": args.native,
        "status": "failed",
        "toucan_sha256": digest(args.toucan.resolve()),
        "commands": [],
        "rustc": [],
        "cases": [],
    }

    def run(
        command: list[str | Path],
        directory: Path = output,
        *,
        reject: str | None = None,
    ) -> str:
        command = list(map(str, command))
        result = subprocess.run(
            command,
            cwd=directory,
            capture_output=True,
            text=True,
            timeout=90,
            check=False,
        )
        evidence["commands"].append(
            {
                "command": command,
                "directory": str(directory),
                "exit_code": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            }
        )
        if reject is not None:
            if result.returncode == 0 or reject not in result.stderr:
                raise RuntimeError(
                    f"expected diagnostic {reject!r}: {command}: {result.stderr}"
                )
        elif result.returncode != 0:
            raise RuntimeError(f"command failed: {command}: {result.stderr}")
        return result.stdout

    try:
        clang = Path(shutil.which(args.clang) or args.clang).resolve()
        run([clang, "--version"])
        rustcs = [
            Path(shutil.which(value) or value).resolve()
            for value in (args.rustc or ["rustc"])
        ]
        for rustc in rustcs:
            evidence["rustc"].append(run([rustc, "--version", "--verbose"]))
        if args.linker:
            linker = args.linker.resolve()
        else:
            sysroot = Path(run([rustcs[-1], "--print", "sysroot"]).strip())
            host = next(
                line.removeprefix("host: ")
                for line in evidence["rustc"][-1].splitlines()
                if line.startswith("host: ")
            )
            linker = (
                sysroot
                / "lib"
                / "rustlib"
                / host
                / "bin"
                / ("rust-lld.exe" if sys.platform == "win32" else "rust-lld")
            )
        if not linker.is_file():
            raise RuntimeError(f"COFF linker not found: {linker}")
        link = [str(linker)] + (
            ["-flavor", "link"] if linker.stem == "rust-lld" else []
        )
        readobj = args.readobj or str(
            clang.with_name(
                "llvm-readobj.exe" if sys.platform == "win32" else "llvm-readobj"
            )
        )
        compile_c = [
            clang,
            "--target=" + TARGET,
            "-std=gnu11",
            "-O2",
            "-ffreestanding",
            "-fno-stack-protector",
            "-c",
        ]
        for name, source in [("probe_a", DLL_A), ("probe_b", DLL_B)]:
            (output / f"{name}.c").write_text(source)
            run([*compile_c, f"{name}.c", "-o", f"{name}.obj"])
            run(
                [
                    *link,
                    "/dll",
                    "/noentry",
                    "/nodefaultlib",
                    f"/out:{name}.dll",
                    f"/implib:{name}.lib",
                    f"{name}.obj",
                ]
            )
        (output / "ordinary.c").write_text("int ordinary=5;\n")
        run([*compile_c, "ordinary.c", "-o", "ordinary.obj"])
        headers = {
            "data_first": DATA + TYPES + FUNCTIONS + ADDRESSES,
            "function_first": FUNCTIONS + DATA + TYPES + ADDRESSES,
            "type_first": TYPES + FUNCTIONS + DATA + ADDRESSES,
        }
        for order, header in headers.items():
            case = output / order
            case.mkdir(exist_ok=True)
            (case / "api.h").write_text(header)
            generate = [
                args.toucan.resolve(),
                "bindgen",
                "api.h",
                "--target",
                TARGET,
                "--rust-target",
                "1.64",
                "--allowlist",
                "a_*",
                "--allowlist",
                "b_*",
                "--allowlist",
                "ordinary",
                "--allowlist",
                "Callback",
            ]
            run(generate, case, reject="matching DLL import library rule")
            run(
                [
                    *generate,
                    "--dll-import-library",
                    "a_*=probe_a",
                    "--dll-import-library",
                    "b_*=probe_b",
                    "--dll-import-library",
                    "a_special=probe_b",
                    "-o",
                    "bindings.rs",
                ],
                case,
            )
            generated = (case / "bindings.rs").read_text()
            assert "unselected" not in generated
            (case / "c_probe.c").write_text(C_PROBE)
            run([*compile_c, "c_probe.c", "-o", "c_probe.obj"], case)
            (case / "consumer.rs").write_text("#![no_std]\n" + RUST_PROBE)
            (case / "native.rs").write_text(
                RUST_PROBE + "\nfn main(){assert_eq!(unsafe{entry()},0);}\n"
            )
            for compiler_index, rustc in enumerate(rustcs):
                for optimization in [0, 3]:
                    destination = case / f"rust{compiler_index}-O{optimization}"
                    destination.mkdir(exist_ok=True)
                    run(
                        [
                            rustc,
                            "--edition=2021",
                            "--crate-type=lib",
                            "--target",
                            TARGET,
                            "--emit=llvm-ir,obj",
                            "-C",
                            f"opt-level={optimization}",
                            "consumer.rs",
                            "--out-dir",
                            destination,
                        ],
                        case,
                    )
                    executable = destination / "consumer.exe"
                    run(
                        [
                            *link,
                            "/entry:entry",
                            "/subsystem:console",
                            "/nodefaultlib",
                            "/out:" + str(executable),
                            destination / "consumer.o",
                            case / "c_probe.obj",
                            output / "ordinary.obj",
                            output / "probe_a.lib",
                            output / "probe_b.lib",
                        ],
                        case,
                    )
                    imports = run([readobj, "--coff-imports", executable], case)
                    for symbol in [
                        "a_value",
                        "b_value",
                        "a_special",
                        "a_function",
                        "b_function",
                        "a_address_data",
                        "a_address_function",
                    ]:
                        assert "Symbol: " + symbol + " " in imports, (symbol, imports)
                    for symbol in ["ordinary", "unselected"]:
                        assert "Symbol: " + symbol + " " not in imports, imports
                    imported = {}
                    for block in re.findall(r"Import \{(.*?)\}", imports, re.DOTALL):
                        library = re.search(r"Name: (\S+)", block).group(1)
                        imported[library] = set(re.findall(r"Symbol: (\S+)", block))
                    assert imported["probe_a.dll"] == {
                        "a_value",
                        "a_function",
                        "a_address_data",
                        "a_address_function",
                    }, imported
                    assert imported["probe_b.dll"] == {
                        "b_value",
                        "b_function",
                        "a_special",
                    }, imported
                    ir = (destination / "consumer.ll").read_text()
                    for symbol in ["a_value", "b_value", "a_special"]:
                        assert any(
                            "@" + symbol + " = external dllimport" in line
                            for line in ir.splitlines()
                        ), (symbol, ir)
                    for symbol in [
                        "a_function",
                        "b_function",
                        "a_address_data",
                        "a_address_function",
                    ]:
                        assert any(
                            line.startswith("declare dllimport ")
                            and "@" + symbol + "(" in line
                            for line in ir.splitlines()
                        ), (symbol, ir)
                    if args.native:
                        native = destination / "native.exe"
                        run(
                            [
                                rustc,
                                "--edition=2021",
                                "--target",
                                TARGET,
                                "-C",
                                f"opt-level={optimization}",
                                "native.rs",
                                "-L",
                                "native=" + str(output),
                                "-C",
                                "link-arg=" + str(case / "c_probe.obj"),
                                "-C",
                                "link-arg=" + str(output / "ordinary.obj"),
                                "-o",
                                native,
                            ],
                            case,
                        )
                        for name in ["probe_a.dll", "probe_b.dll"]:
                            shutil.copy2(output / name, destination / name)
                        run([native], case)
                    evidence["cases"].append(
                        {
                            "header_order": order,
                            "compiler": compiler_index,
                            "optimization": optimization,
                            "bindings_sha256": digest(case / "bindings.rs"),
                            "llvm_sha256": digest(destination / "consumer.ll"),
                            "executable_sha256": digest(executable),
                            "native_executed": args.native,
                            "native_executable_sha256": digest(native)
                            if args.native
                            else None,
                        }
                    )
        quote = output / "quoted-library"
        quote.mkdir(exist_ok=True)
        (quote / "api.h").write_text("__declspec(dllimport) int guarded;\n")
        payload = 'probe");} extern "C" {fn injected();} /*'
        run(
            [
                args.toucan.resolve(),
                "bindgen",
                "api.h",
                "--target",
                TARGET,
                "--rust-target",
                "1.64",
                "--dll-import-library",
                "guarded=" + payload,
                "-o",
                "bindings.rs",
            ],
            quote,
        )
        (quote / "consumer.rs").write_text(
            '#![no_std]\ninclude!("bindings.rs");\nfn injected() {}\n'
        )
        for index, rustc in enumerate(rustcs):
            run(
                [
                    rustc,
                    "--edition=2021",
                    "--crate-type=lib",
                    "--target",
                    TARGET,
                    "--emit=metadata",
                    "consumer.rs",
                    "-o",
                    f"quoted-{index}.rmeta",
                ],
                quote,
            )
        evidence["quoted_library_rust_parsing"] = "passed"
        evidence["status"] = "passed"
    finally:
        (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(
        f"{len(evidence['cases'])} DLL binding consumers linked; native execution={args.native}"
    )


if __name__ == "__main__":
    main()
