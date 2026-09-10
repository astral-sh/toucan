#!/usr/bin/env python3
"""Gate pinned-project API differences against bindgen and independent C evidence.

The raw comparison retains equivalent=false when generators expose different Rust
APIs. A passing gate means every difference belongs to an explicitly checked
category; it does not turn accepted reference differences into exact equivalence.
"""

from __future__ import annotations

import argparse
import copy
import importlib.util
import json
import os
import platform
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

if not __debug__:
    raise RuntimeError(
        "Validation requires Python assertions; unset PYTHONOPTIMIZE and omit -O."
    )

ROOT = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location(
    "verify_corpus", Path(__file__).with_name("verify_corpus.py")
)
assert _spec and _spec.loader
_corpus = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_corpus)
digest, execute, native_target = _corpus.digest, _corpus.execute, _corpus.native_target
PROJECTS = {"libgit2", "sqlite", "zlib", "zstd"}
SENTINELS = {
    "zstd": {"ZSTD_CONTENTSIZE_UNKNOWN": -1, "ZSTD_CONTENTSIZE_ERROR": -2},
    "libgit2": {"GIT_REBASE_NO_OPERATION": -1, "GIT_OBJECT_SIZE_MAX": -1},
}
MACRO_TYPES = {
    "zstd": {
        "ZSTD_MAGICNUMBER": ("u32", "i64", "unsigned int"),
        "ZSTD_MAGIC_DICTIONARY": ("u32", "i64", "unsigned int"),
        "ZSTD_MAGIC_SKIPPABLE_MASK": ("u32", "i64", "unsigned int"),
    },
    "libgit2": {
        "GIT_PATH_LIST_SEPARATOR": ("i32", "u8", "int"),
        "GIT_SUBMODULE_STATUS__INDEX_FLAGS": ("u32", "i32", "unsigned int"),
        "GIT_SUBMODULE_STATUS__IN_FLAGS": ("u32", "i32", "unsigned int"),
        "GIT_SUBMODULE_STATUS__WD_FLAGS": ("u32", "i32", "unsigned int"),
    },
}
DEPRECATED_ALIASES = {
    "git_attr_t": "git_attr_value_t",
    "git_revparse_mode_t": "git_revspec_t",
}
EXTRA_CONSTANTS = {
    "zstd": {"ZSTD_MAX_INPUT_SIZE", "ZSTD_VERSION_STRING"},
    "libgit2": {
        "GIT_ATTR_FALSE_T",
        "GIT_ATTR_TRUE_T",
        "GIT_ATTR_UNSPECIFIED_T",
        "GIT_ATTR_VALUE_T",
        "GIT_BLOB_FILTER_ATTTRIBUTES_FROM_HEAD",
        "GIT_CREDTYPE_DEFAULT",
        "GIT_CREDTYPE_SSH_CUSTOM",
        "GIT_CREDTYPE_SSH_INTERACTIVE",
        "GIT_CREDTYPE_SSH_KEY",
        "GIT_CREDTYPE_SSH_MEMORY",
        "GIT_CREDTYPE_USERNAME",
        "GIT_CREDTYPE_USERPASS_PLAINTEXT",
        "GIT_CVAR_FALSE",
        "GIT_CVAR_INT32",
        "GIT_CVAR_STRING",
        "GIT_CVAR_TRUE",
        "GIT_ERROR_SHA1",
        "GIT_IDXENTRY_EXTENDED",
        "GIT_IDXENTRY_EXTENDED_FLAGS",
        "GIT_IDXENTRY_INTENT_TO_ADD",
        "GIT_IDXENTRY_SKIP_WORKTREE",
        "GIT_IDXENTRY_VALID",
        "GIT_INDEXCAP_FROM_OWNER",
        "GIT_INDEXCAP_IGNORE_CASE",
        "GIT_INDEXCAP_NO_FILEMODE",
        "GIT_INDEXCAP_NO_SYMLINKS",
        "GIT_OBJECT_SIZE_MAX",
        "GIT_OBJ_ANY",
        "GIT_OBJ_BAD",
        "GIT_OBJ_BLOB",
        "GIT_OBJ_COMMIT",
        "GIT_OBJ_OFS_DELTA",
        "GIT_OBJ_REF_DELTA",
        "GIT_OBJ_TAG",
        "GIT_OBJ_TREE",
        "GIT_OID_DEFAULT",
        "GIT_REF_FORMAT_ALLOW_ONELEVEL",
        "GIT_REF_FORMAT_NORMAL",
        "GIT_REF_FORMAT_REFSPEC_PATTERN",
        "GIT_REF_FORMAT_REFSPEC_SHORTHAND",
        "GIT_REF_INVALID",
        "GIT_REF_LISTALL",
        "GIT_REF_OID",
        "GIT_REF_SYMBOLIC",
        "GIT_REVPARSE_MERGE_BASE",
        "GIT_REVPARSE_RANGE",
        "GIT_REVPARSE_SINGLE",
        "GIT_STATUS_OPT_DEFAULTS",
    },
}
VA_LIST_RECORD = "__builtin_va_list_record"
VA_LIST_OFFSETS = {
    "__stack": 0,
    "__gr_top": 8,
    "__vr_top": 16,
    "__gr_offs": 24,
    "__vr_offs": 28,
}


def primitive(name: str) -> dict:
    return {"kind": "primitive", "value": name}


def selected(name: str, patterns: list[str]) -> bool:
    return any(
        name.startswith(pattern[:-1]) if pattern.endswith("*") else name == pattern
        for pattern in patterns
    )


def sqlite_callback_difference(difference: dict) -> bool:
    """Accept only the known nested callback parameter duplication in xDlSym."""
    left, right = difference["toucan"], copy.deepcopy(difference["bindgen"])
    try:
        lf = next(f for f in left["fields"] if f["name"] == "xDlSym")
        rf = next(f for f in right["fields"] if f["name"] == "xDlSym")
        louter = lf["shape"]["value"]["value"]
        router = rf["shape"]["value"]["value"]
        lreturn = louter["result"]["value"]["value"]
        rreturn = router["result"]["value"]["value"]
        expected_return = {
            "abi": "C",
            "unsafe_": True,
            "parameters": [],
            "result": {"kind": "unit"},
            "variadic": False,
        }
        parameters = louter["parameters"]
        if lreturn != expected_return or len(parameters) != 3:
            return False
        expected_parameters = [
            {
                "kind": "pointer",
                "value": {
                    "mutable": True,
                    "pointee": {"kind": "record", "value": "sqlite3_vfs"},
                },
            },
            {
                "kind": "pointer",
                "value": {"mutable": True, "pointee": primitive("void")},
            },
            {
                "kind": "pointer",
                "value": {
                    "mutable": False,
                    "pointee": parameters[2]["value"]["pointee"],
                },
            },
        ]
        if parameters != expected_parameters or parameters[2]["value"][
            "pointee"
        ] not in (primitive("i8"), primitive("u8")):
            return False
        expected_outer = {
            "abi": "C",
            "unsafe_": True,
            "parameters": expected_parameters,
            "result": {
                "kind": "nullable",
                "value": {"kind": "function", "value": expected_return},
            },
            "variadic": False,
        }
        if lf["shape"] != {
            "kind": "nullable",
            "value": {"kind": "function", "value": expected_outer},
        }:
            return False
        if rreturn["parameters"] != router["parameters"]:
            return False
        rreturn["parameters"] = []
        return left == right
    except (KeyError, StopIteration, TypeError):
        return False


def alias_reason(name: str, side: str, report: dict, project: dict) -> str | None:
    inventory = report["inventory"]
    other = "bindgen" if side == "toucan" else "toucan"
    shape = inventory[side]["aliases"][name]
    if not selected(name, project["allowlist"]):
        return "helper alias outside the selected public API"
    if shape.get("kind") == "record":
        pairs = report["record_name_mapping"]
        mapped = pairs if side == "toucan" else {v: k for k, v in pairs.items()}
        counterpart = inventory[other]["records"].get(mapped.get(shape["value"]))
        if counterpart and name in {counterpart["rust_name"], *counterpart["aliases"]}:
            return "typedef represented by the corresponding named Rust record"
    if project["name"] == "libgit2" and side == "toucan":
        canonical = DEPRECATED_ALIASES.get(name)
        if canonical and shape == inventory[other]["aliases"].get(canonical):
            return "deprecated C typedef omitted by bindgen; C type identity checked"
    return None


def classify(report: dict, project: dict) -> tuple[list, list]:
    """Classify exact known differences; the caller must first validate C evidence."""
    accepted, unexpected = [], []
    name = project["name"]
    constants = report["comparisons"]["constants"]
    native = report["comparisons"]["native_constants"]
    for category, comparison in report["comparisons"].items():
        for side in ("toucan", "bindgen"):
            for item in comparison[f"{side}_only"]:
                reason = None
                if category == "aliases":
                    reason = alias_reason(item, side, report, project)
                elif (
                    category == "native_fields"
                    and report.get("c_validated_va_list")
                    and item
                    in {
                        f"{VA_LIST_RECORD}.{field}"
                        for field in (VA_LIST_OFFSETS if side == "toucan" else ["0"])
                    }
                ):
                    reason = "C va_list fields represented by opaque array storage in bindgen; native and C layouts checked"
                elif (
                    category in {"constants", "native_constants"}
                    and side == "toucan"
                    and item in EXTRA_CONSTANTS.get(name, set())
                    and item in constants["toucan_only"]
                    and item in native["toucan_only"]
                ):
                    reason = "additional Toucan constant checked against C; not exact API equivalence"
                entry = {
                    "category": category,
                    "name": item,
                    "difference": f"{side}_only",
                }
                (accepted if reason else unexpected).append(
                    {**entry, **({"reason": reason} if reason else {})}
                )
        for item, difference in comparison["different"].items():
            reason = None
            sentinel = SENTINELS.get(name, {}).get(item)
            if category in {"constants", "native_constants"} and sentinel is not None:
                if constants["different"].get(item) == {
                    "toucan": primitive("u64"),
                    "bindgen": primitive("i32"),
                } and native["different"].get(item) == {
                    "toucan": str((1 << 64) + sentinel),
                    "bindgen": str(sentinel),
                }:
                    reason = "C unsigned 64-bit sentinel emitted as a signed 32-bit constant by bindgen"
            elif category == "constants" and item in MACRO_TYPES.get(name, {}):
                toucan_type, bindgen_type, _ = MACRO_TYPES[name][item]
                if (
                    difference
                    == {
                        "toucan": primitive(toucan_type),
                        "bindgen": primitive(bindgen_type),
                    }
                    and item not in native["different"]
                ):
                    reason = "C macro expression type differs from bindgen's inferred Rust constant type"
            elif (
                category == "record_shapes"
                and name == "sqlite"
                and item == "sqlite3_vfs"
                and sqlite_callback_difference(difference)
            ):
                reason = "C xDlSym returns void (*)(void); bindgen repeats the lookup parameters"
            elif (
                category == "record_shapes"
                and item == VA_LIST_RECORD
                and report.get("c_validated_va_list")
            ):
                reason = "C va_list fields represented by opaque array storage in bindgen; native and C layouts checked"
            entry = {"category": category, "name": item, "difference": difference}
            (accepted if reason else unexpected).append(
                {**entry, **({"reason": reason} if reason else {})}
            )
    for side, records in report["excluded_storage_fields"].items():
        for record, fields in records.items():
            allowed = {
                "toucan": {"__toucan_bits_1", "__toucan_padding_2"},
                "bindgen": {"_bitfield_align_1", "_bitfield_1"},
            }
            known = (
                name == "libgit2"
                and record == "git_commit_create_options"
                and set(fields) == allowed[side]
            )
            entry = {
                "category": "excluded_storage_fields",
                "tool": side,
                "name": record,
                "fields": fields,
            }
            (accepted if known else unexpected).append(
                {
                    **entry,
                    **(
                        {
                            "reason": "bitfield helper storage; C/Rust setter and getter FFI checked"
                        }
                        if known
                        else {}
                    ),
                }
            )
    for key in ("record_mapping_conflicts", "unsupported"):
        value = report[key]
        if any(value.values()) if isinstance(value, dict) else bool(value):
            unexpected.append({"category": key, "difference": value})
    if not report["inputs_unchanged"] or set(report["native_observations"]) != {
        "toucan",
        "bindgen",
    }:
        unexpected.append(
            {
                "category": "validation",
                "difference": "missing native observations or changed inputs",
            }
        )
    return accepted, unexpected


def validate_va_list(
    report: dict, project: dict, args, directory: Path, commands: list
) -> dict | None:
    """Check the concrete AArch64 va_list representation before accepting opaque storage."""
    if args.target != "aarch64-unknown-linux-gnu" or project["name"] not in {
        "sqlite",
        "zlib",
    }:
        return None
    difference = report["comparisons"]["record_shapes"]["different"].get(VA_LIST_RECORD)
    pointer = {
        "kind": "pointer",
        "value": {"mutable": True, "pointee": primitive("void")},
    }
    expected = {
        "toucan": {
            "kind": "struct",
            "opaque": False,
            "fields": [
                {"name": field, "shape": pointer if offset < 24 else primitive("i32")}
                for field, offset in VA_LIST_OFFSETS.items()
            ],
        },
        "bindgen": {
            "kind": "struct",
            "opaque": False,
            "fields": [
                {
                    "name": "0",
                    "shape": {
                        "kind": "array",
                        "value": {"element": primitive("u64"), "length": "4"},
                    },
                }
            ],
        },
    }
    if difference != expected:
        return None
    other = report["record_name_mapping"].get(VA_LIST_RECORD)
    for side, record in (("toucan", VA_LIST_RECORD), ("bindgen", other)):
        observed = report["native_observations"][side]
        if observed["records"].get(record) != {"size": 32, "alignment": 8}:
            return None
        offsets = VA_LIST_OFFSETS if side == "toucan" else {"0": 0}
        if any(
            observed["fields"].get(f"{record}.{field}") != offset
            for field, offset in offsets.items()
        ):
            return None
    source = directory / "va-list-oracle.c"
    lines = [
        f"#include {json.dumps(project['header'])}",
        '_Static_assert(__builtin_types_compatible_p(va_list, __builtin_va_list), "header va_list matches the compiler builtin");',
        '_Static_assert(sizeof(__builtin_va_list) == 32, "va_list size");',
        '_Static_assert(_Alignof(__builtin_va_list) == 8, "va_list alignment");',
    ]
    for field, offset in VA_LIST_OFFSETS.items():
        c_type = "void *" if offset < 24 else "int"
        lines.extend(
            [
                f'_Static_assert(__builtin_offsetof(__builtin_va_list, {field}) == {offset}, "{field} offset");',
                f'_Static_assert(_Generic(((__builtin_va_list){{0}}).{field}, {c_type}: 1, default: 0), "{field} type");',
            ]
        )
    source.write_text("\n".join(lines) + "\n")
    command = [
        args.cc,
        "-std=c11",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-fsyntax-only",
        f"--sysroot={args.sysroot}",
        str(source),
    ]
    for include in project["include_dirs"]:
        command += ["-I", include]
    execute(command, directory, "compile-va-list-oracle", commands, args.timeout)
    return {
        "status": "passed",
        "source_sha256": digest(source),
        "size": 32,
        "alignment": 8,
        "field_offsets": VA_LIST_OFFSETS,
    }


def validate_c_evidence(
    evidence: dict, project: dict, args, bindings: Path, metadata: dict
) -> dict:
    """Tie a passing independent C/FFI run to the exact generated binding bytes."""
    if evidence.get("status") != "passed" or evidence.get("target") != args.target:
        raise RuntimeError("C evidence must pass for the current native target")
    result = next(
        (p for p in evidence["projects"] if p["name"] == project["name"]), None
    )
    if result is None or result.get("status") != "passed" or result.get("differences"):
        raise RuntimeError(f"missing passing C evidence for {project['name']}")
    expected = {
        "target": args.target,
        "toucan_sha256": digest(args.toucan),
        "bindings_sha256": digest(bindings),
        "header_sha256": digest(Path(project["header"])),
        "commit": project["commit"],
        "archive_sha256": project["sha256"],
        "library_sha256": project["library_sha256"],
    }
    for key, value in expected.items():
        if result.get(key) != value:
            raise RuntimeError(
                f"C evidence does not match current {project['name']} {key}"
            )
    for path, checksum in project["library_sha256"].items():
        if digest(Path(path)) != checksum:
            raise RuntimeError(f"library changed after C/FFI validation: {path}")
    dependencies = {
        path: digest(Path(path))
        for path in metadata["dependencies"]
        if Path(path).is_file()
    }
    if result.get("dependency_sha256") != dependencies:
        raise RuntimeError(
            "C evidence header dependencies differ from the current generation"
        )
    if result["bindings_report"].get("skipped_declarations") or metadata.get(
        "skipped_declarations"
    ):
        raise RuntimeError("selected declarations were skipped")
    return {
        "evidence_sha256": digest(args.c_evidence),
        "project": project["name"],
        "bindings_sha256": expected["bindings_sha256"],
        "comparisons": result["comparisons"],
        "coverage": result["coverage"],
    }


def verify(project: dict, args, evidence: dict) -> dict:
    name = project["name"]
    directory = args.output / name
    directory.mkdir(parents=True, exist_ok=True)
    commands = []
    result = {"name": name, "status": "failed", "commands": commands}
    try:
        header = str(Path(project["header"]).resolve())
        bindings = directory / "bindings.rs"
        reference = directory / "bindgen.rs"
        metadata_path = directory / "bindings-report.json"
        options = [header, "--target", args.target, "--sysroot", str(args.sysroot)]
        includes = [*project["include_dirs"], *args.include_dir]
        for include in includes:
            options += ["-I", include]
        toucan_command = [
            str(args.toucan),
            "bindgen",
            *options,
            "--output",
            str(bindings),
            "--report",
            str(metadata_path),
        ]
        bindgen_command = [
            str(args.bindgen),
            header,
            "--no-doc-comments",
            "--no-layout-tests",
            "--formatter",
            "none",
            "--no-prepend-enum-name",
            "--default-macro-constant-type",
            "signed",
            "--output",
            str(reference),
        ]
        for pattern in project["allowlist"]:
            toucan_command += ["--allowlist", pattern]
            regex = (
                re.escape(pattern[:-1]) + ".*"
                if pattern.endswith("*")
                else re.escape(pattern)
            )
            for category in ("type", "function", "var"):
                bindgen_command += [f"--allowlist-{category}", regex]
        bindgen_command += [
            "--",
            "-x",
            "c",
            "-std=c11",
            f"--target={args.target}",
            f"--sysroot={args.sysroot}",
        ]
        for include in includes:
            bindgen_command += ["-I", include]
        execute(toucan_command, directory, "generate-toucan", commands, args.timeout)
        metadata = json.loads(metadata_path.read_text())
        result["c_evidence"] = validate_c_evidence(
            evidence, project, args, bindings, metadata
        )
        dependencies = {
            path: digest(Path(path))
            for path in metadata["dependencies"]
            if Path(path).is_file()
        }
        execute(bindgen_command, directory, "generate-bindgen", commands, args.timeout)
        result["enum_constant_names"] = sorted(
            constant["c_name"]
            for group in metadata["enum_constants"]
            for constant in group["emitted"]
        )
        oracle = ROOT / "corpus" / "oracles" / f"{name}.c"
        if oracle.exists():
            source = directory / "oracle.c"
            lines = [
                f"#include {json.dumps(header)}",
                f"#include {json.dumps(str(oracle))}",
            ]
            if name == "libgit2":
                lines += [
                    f'_Static_assert(__builtin_types_compatible_p({alias}, {canonical}), "deprecated typedef identity");'
                    for alias, canonical in DEPRECATED_ALIASES.items()
                ]
            lines += [
                f'_Static_assert(_Generic(({constant}), {c_type}: 1, default: 0), "{constant} has C {c_type} type");'
                for constant, (_, _, c_type) in MACRO_TYPES.get(name, {}).items()
            ]
            source.write_text("\n".join(lines) + "\n")
            command = [
                args.cc,
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
                str(source),
            ]
            command += (
                ["-isysroot", str(args.sysroot)]
                if platform.system() == "Darwin"
                else [f"--sysroot={args.sysroot}"]
            )
            for include in project["include_dirs"]:
                command += ["-I", include]
            execute(command, directory, "compile-oracles", commands, args.timeout)
            result["oracle_sha256"] = digest(oracle)
            result["oracle_source_sha256"] = digest(source)
            result["oracle_status"] = "passed"
        comparison_path = args.output / f"{name}-comparison.json"
        execute(
            [
                sys.executable,
                str(ROOT / "scripts/compare_bindings.py"),
                "--toucan-bindings",
                str(bindings),
                "--bindgen-bindings",
                str(reference),
                "--target",
                args.target,
                "--output",
                str(comparison_path),
                "--analyzer",
                str(args.analyzer),
                "--rustc",
                args.rustc,
            ],
            directory,
            "compare",
            commands,
            args.timeout,
        )
        report = json.loads(comparison_path.read_text())
        va_list_proof = validate_va_list(report, project, args, directory, commands)
        if va_list_proof:
            result["va_list_proof"] = va_list_proof
            report["c_validated_va_list"] = True
        for tool, inventory in report["inventory"].items():
            (args.output / f"{name}-{tool}-api.json").write_text(
                json.dumps(inventory, indent=2) + "\n"
            )
        if dependencies != {path: digest(Path(path)) for path in dependencies}:
            raise RuntimeError("header dependencies changed during comparison")
        if args.binary_sha256 != {
            tool: digest(getattr(args, tool)) for tool in args.binary_sha256
        }:
            raise RuntimeError("a tool executable changed during comparison")
        coverage = result["c_evidence"]["coverage"]
        if coverage["integer_constants"] + coverage["string_constants"] != len(
            report["inventory"]["toucan"]["constants"]
        ):
            raise RuntimeError("C evidence did not cover every generated constant")
        accepted, unexpected = classify(report, project)
        result.update(
            {
                "comparison": str(comparison_path),
                "comparison_sha256": digest(comparison_path),
                "bindings": {"toucan": str(bindings), "bindgen": str(reference)},
                "input_sha256": report["input_sha256"],
                "exact_equivalence": report["equivalent"],
                "accepted_differences": accepted,
                "unexpected_differences": unexpected,
                "counts": {
                    category: {key: comparison[key] for key in ("common", "equal")}
                    for category, comparison in report["comparisons"].items()
                },
                "status": "failed" if unexpected else "passed",
                "qualification": "C-validated reference differences"
                if accepted
                else "equivalent generated APIs and native Rust observations",
            }
        )
    except (
        OSError,
        RuntimeError,
        ValueError,
        KeyError,
        subprocess.SubprocessError,
    ) as error:
        result["error"] = str(error)
    (directory / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(
        f"{name}: {result['status']}"
        + (
            f"\n{result['error']}"
            if "error" in result
            else f"; {len(result['accepted_differences'])} classified, {len(result['unexpected_differences'])} unexpected differences"
        ),
        flush=True,
    )
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepared", type=Path, required=True)
    parser.add_argument("--toucan", type=Path, required=True)
    parser.add_argument("--bindgen", type=Path, required=True)
    parser.add_argument("--analyzer", type=Path, required=True)
    parser.add_argument(
        "--c-evidence",
        type=Path,
        required=True,
        help="passing verify_corpus.py evidence for identical binding bytes",
    )
    parser.add_argument("--target", default=native_target())
    parser.add_argument("--sysroot", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--project", action="append", choices=sorted(PROJECTS))
    parser.add_argument("--include-dir", action="append", default=[])
    parser.add_argument("--cc", default=os.environ.get("CC", "cc"))
    parser.add_argument("--rustc", default=os.environ.get("RUSTC", "rustc"))
    parser.add_argument("--timeout", type=int, default=300)
    args = parser.parse_args()
    if args.target != native_target():
        parser.error(
            "the comparison executes native probes; --target must match the host"
        )
    for key in (
        "prepared",
        "toucan",
        "bindgen",
        "analyzer",
        "c_evidence",
        "sysroot",
        "output",
    ):
        setattr(args, key, getattr(args, key).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    args.binary_sha256 = {
        tool: digest(getattr(args, tool)) for tool in ("toucan", "bindgen", "analyzer")
    }
    resource = subprocess.check_output(
        [args.cc, "-print-file-name=include"], text=True
    ).strip()
    if Path(resource).is_dir():
        args.include_dir.append(str(Path(resource).resolve()))
    prepared = json.loads(args.prepared.read_text())
    evidence = json.loads(args.c_evidence.read_text())
    expected = set(args.project or PROJECTS)
    projects = [p for p in prepared["projects"] if p["name"] in expected]
    missing = sorted(expected - {p["name"] for p in projects})
    results = [verify(project, args, evidence) for project in projects]
    report = {
        "schema_version": 1,
        "recorded_at": datetime.now(timezone.utc).isoformat(),
        "target": args.target,
        "platform": platform.platform(),
        "sysroot": str(args.sysroot),
        "tool_sha256": args.binary_sha256,
        "prepared_sha256": digest(args.prepared),
        "c_evidence_sha256": digest(args.c_evidence),
        "missing_projects": missing,
        "projects": results,
        "status": "passed"
        if not missing and all(p["status"] == "passed" for p in results)
        else "failed",
        "exact_equivalence": bool(results)
        and not missing
        and all(p.get("exact_equivalence", False) for p in results),
        "limitations": [
            "Accepted differences remain visible; passing this regression gate does not imply identical generated Rust APIs.",
            "C constant types and values and selected native FFI calls come from matching independent corpus evidence.",
            "Every complete record and ordinary field is compared between generators; run probe_record_layouts.py for independent C verification of all of them.",
            "The selected libraries and target do not establish correctness for every C program or ABI.",
        ],
    }
    path = args.output / "evidence.json"
    path.write_text(json.dumps(report, indent=2) + "\n")
    print(path)
    return int(report["status"] != "passed")


if __name__ == "__main__":
    raise SystemExit(main())
