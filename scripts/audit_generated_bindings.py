#!/usr/bin/env python3
"""Check generated C headers and invalid mutations against GCC, Clang, and Rust."""

import argparse
import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from audit_c_testsuite import accepted, native_target, run, tool_failure


def generated_case(seed):
    """Generate only defined integer expressions and ordinary, bounded C records."""
    rng = random.Random(seed)
    scalars = ["char", "short", "unsigned int", "long", "long long", "float", "double"]
    count = rng.randint(1, 8)
    a, b, c = (rng.randrange(1, 1000) for _ in range(3))
    expression = str(a)
    for _ in range(rng.randint(2, 6)):
        operator = rng.choice(["+", "^", "|", "&"])
        expression = f"({expression} {operator} {rng.randrange(1, 1000)})"
    suffix = rng.choice(["U", "UL", "ULL"])
    source = f"""typedef struct sample_inner {{
    {rng.choice(scalars)} first;
    {rng.choice(scalars)} array[{count}];
}} sample_inner;
typedef union sample_union {{
    {rng.choice(scalars)} number;
    char bytes[{rng.randint(1, 19)}];
}} sample_union;
typedef struct sample_outer {{
    {rng.choice(scalars)} tag;
    sample_inner inner;
    sample_union choice;
    unsigned int values[{rng.randint(1, 8)}];
}} sample_outer;
enum sample_enum {{ CASE_A = {expression}, CASE_B = CASE_A ^ {b} }};
#define VALUE (({a}{suffix} << {rng.randrange(1, 32)}) + {b}{suffix} * {c}{suffix})
#define CONDITIONAL (VALUE > {c}ULL ? VALUE ^ {b}ULL : {a}ULL)
#define LAYOUT_VALUE (sizeof(sample_outer) + _Alignof(sample_inner))
int sample_consume(sample_outer value);
"""
    # Mutate generated declarations, preserving the rest of each valid header.
    mutations = {
        "duplicate-member": source.replace("sample_inner inner;", "sample_inner tag;"),
        "negative-array": source.replace(f"array[{count}]", f"array[-{count}]"),
        "conflicting-typedef": source + "typedef unsigned long sample_outer;\n",
        "nonconstant-enum": "int sample_dynamic(void);\n"
        + source.replace(f"CASE_A = {expression}", "CASE_A = sample_dynamic()"),
        "wide-bitfield": source
        + f"struct invalid_bits {{ unsigned char value : {rng.randint(9, 16)}; }};\n",
        "wrong-argument": source
        + f"int invalid_call(void) {{ return sample_consume({a}); }}\n",
        "const-assignment": source
        + f"void invalid_write(void) {{ const int value = {a}; value = {b}; }}\n",
        "incomplete-member": source
        + "struct invalid_record { struct incomplete value; };\n",
    }
    return source, mutations


def probes():
    """Share names, never expected values or layouts, between C and Rust probes."""
    c = [
        '#include "input.h"',
        "#include <stddef.h>",
        "#include <stdio.h>",
        "int main(void) {",
    ]
    rust = [
        "#![allow(dead_code, non_camel_case_types, non_upper_case_globals, non_snake_case)]",
        'mod b { include!("bindings.rs"); }',
        "trait Signed { const SIGNED: bool; }",
        "macro_rules! signed { ($value:expr; $($ty:ty),*) => { $(impl Signed for $ty { const SIGNED: bool = $value; })* }; }",
        "signed!(true; i8, i16, i32, i64, i128, isize);",
        "signed!(false; u8, u16, u32, u64, u128, usize);",
        "fn is_signed<T: Signed>(_: T) -> bool { T::SIGNED }",
        "fn main() {",
    ]
    expressions = []
    for name in ("CASE_A", "CASE_B", "VALUE", "CONDITIONAL", "LAYOUT_VALUE"):
        expressions.append((name, name, f"b::{name}"))
    for name in ("VALUE", "CONDITIONAL", "LAYOUT_VALUE"):
        expressions.extend(
            [
                (
                    f"{name}.width",
                    f"sizeof({name})",
                    f"::core::mem::size_of_val(&b::{name})",
                ),
                (
                    f"{name}.signed",
                    f"_Generic(({name}), unsigned int: 0, unsigned long: 0, unsigned long long: 0, default: 1)",
                    f"is_signed(b::{name})",
                ),
            ]
        )
    for record, fields in (
        ("sample_inner", ("first", "array")),
        ("sample_union", ("number", "bytes")),
        ("sample_outer", ("tag", "inner", "choice", "values")),
    ):
        expressions.extend(
            [
                (
                    f"{record}.size",
                    f"sizeof({record})",
                    f"::core::mem::size_of::<b::{record}>()",
                ),
                (
                    f"{record}.align",
                    f"_Alignof({record})",
                    f"::core::mem::align_of::<b::{record}>()",
                ),
            ]
        )
        expressions.extend(
            (
                f"{record}.{field}",
                f"offsetof({record}, {field})",
                f"::core::mem::offset_of!(b::{record}, {field})",
            )
            for field in fields
        )
    for label, c_expression, rust_expression in expressions:
        c.append(f'printf("{label}=%llu\\n", (unsigned long long)({c_expression}));')
        rust.append(f'println!("{label}={{}}", ({rust_expression}) as u64);')
    c.append("return 0; }")
    rust.append("}")
    return "\n".join(c) + "\n", "\n".join(rust) + "\n"


def require_result(result, expected):
    diagnostic = Path(result["stderr"]).read_text(errors="replace")
    if tool_failure(result):
        raise RuntimeError(f"tool failure: {result['command']}: {diagnostic[-3000:]}")
    if accepted(result) != expected:
        raise RuntimeError(
            f"expected {'acceptance' if expected else 'rejection'}: {result['command']}: {diagnostic[-3000:]}"
        )
    if not expected and (result["exit_code"] != 1 or not diagnostic.strip()):
        raise RuntimeError(
            f"invalid rejection: {result['command']}: {diagnostic[-3000:]}"
        )


def audit(args):
    target = native_target()
    if not target.endswith("-unknown-linux-gnu"):
        raise RuntimeError("the generated binding audit requires native GNU Linux")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {
        "seed": args.seed,
        "count": args.count,
        "target": target,
        "status": "failed",
        "cases": [],
    }
    rustc = ["rustc", *([f"+{args.toolchain}"] if args.toolchain else [])]
    commands = []
    toucan = str(args.toucan.resolve())

    def execute(command, directory, name, expected=True):
        result = run(command, directory, name, args.timeout)
        commands.append(result)
        require_result(result, expected)
        return Path(result["stdout"]).read_text()

    try:
        report["versions"] = {
            "gcc": execute([args.gcc, "--version"], output, "gcc-version"),
            "clang": execute([args.clang, "--version"], output, "clang-version"),
            "rustc": execute([*rustc, "-vV"], output, "rustc-version"),
            "toucan": execute([toucan, "--version"], output, "toucan-version"),
        }
        if (
            "Free Software Foundation" not in report["versions"]["gcc"]
            or "clang" not in report["versions"]["clang"].lower()
        ):
            raise RuntimeError("the acceptance oracles must be GNU GCC and Clang")
        if f"host: {target}" not in report["versions"]["rustc"].splitlines():
            raise RuntimeError(f"the Rust compiler must have native host {target}")
        for index in range(args.count):
            seed = args.seed + index
            directory = output / str(seed)
            directory.mkdir()
            source, mutations = generated_case(seed)
            (directory / "input.h").write_text(source)
            c, rust = probes()
            (directory / "probe.c").write_text(c)
            (directory / "probe.rs").write_text(rust)
            case = {"seed": seed, "mutations": list(mutations), "status": "failed"}
            report["cases"].append(case)
            observations = []
            for profile, compiler in (("gcc", args.gcc), ("clang", args.clang)):
                c_flags = [compiler, "-std=c11", "-pedantic-errors", "-x", "c"]
                execute(
                    [*c_flags, "-fsyntax-only", "input.h"],
                    directory,
                    f"{profile}-accept",
                )
                frontend = [
                    "--target",
                    report["target"],
                    "--compiler",
                    profile,
                    "--std=c11",
                ]
                execute(
                    [toucan, "check", *frontend, "input.h"],
                    directory,
                    f"{profile}-toucan-accept",
                )
                for mutation, invalid_source in mutations.items():
                    name = f"{mutation}.h"
                    (directory / name).write_text(invalid_source)
                    execute(
                        [*c_flags, "-fsyntax-only", name],
                        directory,
                        f"{profile}-{mutation}",
                        False,
                    )
                    execute(
                        [toucan, "check", *frontend, name],
                        directory,
                        f"{profile}-toucan-{mutation}",
                        False,
                    )
                execute(
                    [
                        toucan,
                        "bindgen",
                        *frontend,
                        "input.h",
                        "--deny-skipped-macros",
                        "--output",
                        "bindings.rs",
                    ],
                    directory,
                    f"{profile}-generate",
                )
                # Preserve both profile outputs even when a later stage fails.
                (directory / f"{profile}-bindings.rs").write_bytes(
                    (directory / "bindings.rs").read_bytes()
                )
                execute(
                    [
                        *rustc,
                        "--edition=2021",
                        "--crate-name",
                        "generated_oracle",
                        "-D",
                        "improper_ctypes",
                        "probe.rs",
                        "-o",
                        f"{profile}-rust",
                    ],
                    directory,
                    f"{profile}-rust-compile",
                )
                observations.append(
                    execute(
                        [str(directory / f"{profile}-rust")],
                        directory,
                        f"{profile}-rust-run",
                    )
                )
                for optimization in ("0", "2"):
                    executable = f"{profile}-c-O{optimization}"
                    execute(
                        [*c_flags, f"-O{optimization}", "probe.c", "-o", executable],
                        directory,
                        f"{executable}-compile",
                    )
                    observations.append(
                        execute(
                            [str(directory / executable)],
                            directory,
                            f"{executable}-run",
                        )
                    )
            if len(set(observations)) != 1:
                raise RuntimeError(
                    f"C/Rust values or layouts differ for seed {seed}; see {directory}"
                )
            case["observations"] = observations[0].splitlines()
            case["status"] = "passed"
            print(
                f"seed {seed}: generated bindings and {len(mutations)} rejected mutations match both compilers",
                flush=True,
            )
        report["status"] = "passed"
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        report["commands"] = commands
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=20260910)
    parser.add_argument("--count", type=int, default=16)
    parser.add_argument("--gcc", default="gcc")
    parser.add_argument("--clang", default="clang")
    parser.add_argument(
        "--toolchain", help="optional rustup toolchain for generated Rust"
    )
    parser.add_argument("--timeout", type=float, default=30)
    args = parser.parse_args()
    if not 1 <= args.count <= 128 or not 0 < args.timeout <= 120:
        parser.error(
            "count must be 1..128 and timeout must be positive and at most 120 seconds"
        )
    audit(args)


if __name__ == "__main__":
    main()
