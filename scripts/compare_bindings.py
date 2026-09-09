#!/usr/bin/env python3
"""Compare generated Rust bindings through parsed APIs and compiled native probes.

Inputs can be two existing Rust files, or a benchmark record containing the exact
Toucan and bindgen generation commands. This program performs no timing. It
reports every difference and exits unsuccessfully when --require-equivalent is
set. Generated naming choices and private bitfield storage are recorded explicitly.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import platform
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
RUST_KEYWORDS = {
    "as",
    "break",
    "const",
    "continue",
    "crate",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
    "async",
    "await",
    "dyn",
    "abstract",
    "become",
    "box",
    "do",
    "final",
    "macro",
    "override",
    "priv",
    "typeof",
    "unsized",
    "virtual",
    "yield",
    "try",
    "gen",
}


def run(command: list[str], *, timeout: int = 120) -> str:
    result = subprocess.run(
        command, capture_output=True, text=True, timeout=timeout, check=False
    )
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {command!r}\n{result.stderr}"
        )
    return result.stdout


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compare_maps(left: dict, right: dict) -> dict:
    common = left.keys() & right.keys()
    different = {
        key: {"toucan": left[key], "bindgen": right[key]}
        for key in sorted(common)
        if left[key] != right[key]
    }
    return {
        "common": len(common),
        "equal": len(common) - len(different),
        "toucan_only": sorted(left.keys() - right.keys()),
        "bindgen_only": sorted(right.keys() - left.keys()),
        "different": different,
    }


def matches(comparison: dict) -> bool:
    return not any(
        comparison[key] for key in ("toucan_only", "bindgen_only", "different")
    )


def shape_pairs(left: Any, right: Any):
    """Yield nominal record pairs occupying corresponding positions in a type."""
    if isinstance(left, dict) and isinstance(right, dict):
        if left.get("kind") == right.get("kind") == "record":
            yield left["value"], right["value"]
        elif left.keys() == right.keys():
            for key in left:
                yield from shape_pairs(left[key], right[key])
    elif isinstance(left, list) and isinstance(right, list) and len(left) == len(right):
        for a, b in zip(left, right):
            yield from shape_pairs(a, b)


def field_name(name: str, other_names: set[str]) -> str:
    # bindgen escapes Rust keywords with a suffix; Toucan uses raw identifiers.
    # Only normalize when the counterpart actually contains that keyword.
    if name.endswith("_") and name[:-1] in RUST_KEYWORDS and name[:-1] in other_names:
        return name[:-1]
    return name


def record_pairs(left: dict, right: dict) -> tuple[dict[str, str], list[dict]]:
    """Match records through shared typedefs and corresponding API access paths.

    The pairing is bijective. Conflicting mappings are reported and prevent a
    successful comparison. Fields and layouts must still match after pairing.
    """
    pairs: dict[str, str] = {}
    reverse: dict[str, str] = {}
    conflicts = []

    def add(a: str, b: str, reason: str):
        if a not in left["records"] or b not in right["records"]:
            return
        if a in pairs and pairs[a] != b or b in reverse and reverse[b] != a:
            entry = {"toucan": a, "bindgen": b, "reason": reason}
            if entry not in conflicts:
                conflicts.append(entry)
            return
        pairs[a] = b
        reverse[b] = a

    for key in left["records"].keys() & right["records"].keys():
        add(key, key, "shared record name")
    for a, lrecord in left["records"].items():
        lnames = {a, lrecord["rust_name"], *lrecord["aliases"]}
        for b, rrecord in right["records"].items():
            if lnames & {b, rrecord["rust_name"], *rrecord["aliases"]}:
                add(a, b, "shared typedef or record name")
    for category in ("functions", "globals", "aliases"):
        for key in left[category].keys() & right[category].keys():
            ltype = left[category][key]
            rtype = right[category][key]
            if category != "aliases":
                ltype, rtype = ltype["shape"], rtype["shape"]
            for a, b in shape_pairs(ltype, rtype):
                add(a, b, f"{category}.{key}")
    visited = set()
    while new_pairs := pairs.keys() - visited:
        for a in sorted(new_pairs):
            visited.add(a)
            b = pairs[a]
            lf = {f["name"]: f for f in left["records"][a]["fields"]}
            rf = {
                field_name(f["name"], set(lf)): f for f in right["records"][b]["fields"]
            }
            for field in lf.keys() & rf.keys():
                for c, d in shape_pairs(lf[field]["shape"], rf[field]["shape"]):
                    add(c, d, f"record {a}.{field}")
    return pairs, conflicts


def rename_records(value: Any, mapping: dict[str, str]) -> Any:
    if isinstance(value, dict):
        if value.get("kind") == "record":
            return {
                "kind": "record",
                "value": mapping.get(value["value"], value["value"]),
            }
        return {key: rename_records(v, mapping) for key, v in value.items()}
    if isinstance(value, list):
        return [rename_records(v, mapping) for v in value]
    return value


def normalize(
    left: dict, right: dict, pairs: dict[str, str]
) -> tuple[dict, dict, list]:
    a = copy.deepcopy(left)
    b = rename_records(copy.deepcopy(right), {b: a for a, b in pairs.items()})
    b["records"] = {
        next((a for a, v in pairs.items() if v == k), k): v
        for k, v in b["records"].items()
    }
    field_renames = []
    for key in a["records"].keys() & b["records"].keys():
        names = {field["name"] for field in a["records"][key]["fields"]}
        for field in b["records"][key]["fields"]:
            canonical = field_name(field["name"], names)
            if canonical != field["name"]:
                field_renames.append(
                    {"record": key, "bindgen": field["name"], "toucan": canonical}
                )
                field["name"] = canonical
    return a, b, field_renames


def record_shape(record: dict) -> dict:
    return {
        "kind": record["kind"],
        "opaque": record["opaque"],
        "fields": [{"name": f["name"], "shape": f["shape"]} for f in record["fields"]],
    }


def parse_probe(output: str) -> dict:
    result = {"records": {}, "fields": {}, "constants": {}}
    for line in output.splitlines():
        parts = line.split("\t")
        if parts[0] == "record":
            _, name, size, alignment = parts
            result["records"][name] = {"size": int(size), "alignment": int(alignment)}
        elif parts[0] == "field":
            _, record, field, offset = parts
            result["fields"][f"{record}.{field}"] = int(offset)
        elif parts[0] == "constant":
            _, name, value = parts
            result["constants"][name] = value
        else:
            raise ValueError(f"unknown probe output: {line!r}")
    return result


def native_probe(
    source: Path, output: Path, target: str, rustc: str, edition: str
) -> dict:
    # Explicit target is required even on the host: this also exercises the
    # generated target guard. Executing a foreign-architecture binary will fail.
    run(
        [
            rustc,
            f"--edition={edition}",
            "--crate-name",
            "binding_probe",
            "--target",
            target,
            str(source),
            "-o",
            str(output),
        ]
    )
    return parse_probe(run([str(output)]))


def normalized_probe(probe: dict, mapping: dict[str, str], renames: list[dict]) -> dict:
    result = copy.deepcopy(probe)
    result["records"] = {mapping.get(k, k): v for k, v in probe["records"].items()}
    fields = {}
    field_map = {(r["record"], r["bindgen"]): r["toucan"] for r in renames}
    for key, value in probe["fields"].items():
        record, field = key.rsplit(".", 1)
        record = mapping.get(record, record)
        field = field_map.get((record, field), field)
        fields[f"{record}.{field}"] = value
    result["fields"] = fields
    return result


def generate_from_record(
    path: Path, directory: Path, target: str
) -> tuple[dict, dict[str, Path], dict]:
    record = json.loads(path.read_text())
    if record["target"] != target:
        raise ValueError("benchmark record target does not match --target")
    dependencies = {str(p): digest(p) for p in map(Path, record["dependency_sha256"])}
    commands = copy.deepcopy(record["commands"])
    bindgen = commands["bindgen"]
    separator = bindgen.index("--")
    options = []
    if "--no-prepend-enum-name" not in bindgen:
        options.append("--no-prepend-enum-name")
    if "--default-macro-constant-type" not in bindgen:
        options += ["--default-macro-constant-type", "signed"]
    bindgen[separator:separator] = options
    paths = {}
    for tool, command in commands.items():
        paths[tool] = directory / f"{tool}.rs"
        paths[tool].write_text(run(command))
    after = {str(p): digest(p) for p in map(Path, dependencies)}
    if after != dependencies:
        raise RuntimeError(
            "header dependencies changed while generating comparison outputs"
        )
    return commands, paths, dependencies


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--toucan-bindings", type=Path)
    parser.add_argument("--bindgen-bindings", type=Path)
    parser.add_argument("--benchmark-record", type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--analyzer", type=Path)
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument(
        "--edition",
        choices=("2015", "2018", "2021", "2024"),
        default="2024",
        help="Rust edition for the native probe including the generated bindings",
    )
    parser.add_argument("--require-equivalent", action="store_true")
    parser.add_argument("--skip-native-probes", action="store_true")
    args = parser.parse_args()
    if args.benchmark_record:
        if args.toucan_bindings or args.bindgen_bindings:
            parser.error("use a benchmark record or two existing binding files")
    elif not (args.toucan_bindings and args.bindgen_bindings):
        parser.error(
            "provide --benchmark-record or both --toucan-bindings and --bindgen-bindings"
        )
    if args.analyzer:
        analyzer = args.analyzer.resolve()
    else:
        manifest = ROOT / "tools/binding_compare/Cargo.toml"
        run(["cargo", "build", "--locked", "--manifest-path", str(manifest)])
        metadata = json.loads(
            run(
                [
                    "cargo",
                    "metadata",
                    "--no-deps",
                    "--format-version=1",
                    "--manifest-path",
                    str(manifest),
                ]
            )
        )
        analyzer = Path(metadata["target_directory"]) / "debug/toucan-binding-compare"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix="toucan-compare-", dir=args.output.parent
    ) as tmp:
        directory = Path(tmp).resolve()
        if args.benchmark_record:
            commands, paths, dependencies = generate_from_record(
                args.benchmark_record, directory, args.target
            )
        else:
            commands = {}
            dependencies = {}
            paths = {
                "toucan": args.toucan_bindings.resolve(),
                "bindgen": args.bindgen_bindings.resolve(),
            }
        before_hashes = {tool: digest(path) for tool, path in paths.items()}
        apis, probes = {}, {}
        for tool, path in paths.items():
            probe_path = directory / f"{tool}-probe.rs"
            apis[tool] = json.loads(
                run([str(analyzer), str(path), args.target, str(probe_path)])
            )
            if not args.skip_native_probes:
                probes[tool] = native_probe(
                    probe_path,
                    directory / f"{tool}-probe",
                    args.target,
                    args.rustc,
                    args.edition,
                )
        pairs, conflicts = record_pairs(apis["toucan"], apis["bindgen"])
        a, b, field_renames = normalize(apis["toucan"], apis["bindgen"], pairs)
        comparisons = {
            key: compare_maps(
                {n: v["shape"] for n, v in a[key].items()},
                {n: v["shape"] for n, v in b[key].items()},
            )
            for key in ("functions", "globals", "constants")
        }
        comparisons["aliases"] = compare_maps(a["aliases"], b["aliases"])
        comparisons["record_shapes"] = compare_maps(
            {k: record_shape(v) for k, v in a["records"].items()},
            {k: record_shape(v) for k, v in b["records"].items()},
        )
        if probes:
            bp = normalized_probe(
                probes["bindgen"], {v: k for k, v in pairs.items()}, field_renames
            )
            for category in ("records", "fields", "constants"):
                comparisons[f"native_{category}"] = compare_maps(
                    probes["toucan"][category], bp[category]
                )
        excluded = {
            tool: {
                name: r["excluded_fields"]
                for name, r in api["records"].items()
                if r["excluded_fields"] and not r["opaque"]
            }
            for tool, api in apis.items()
        }
        report = {
            "schema_version": 1,
            "measured_at_utc": datetime.now(timezone.utc).isoformat(),
            "analyzer_sha256": digest(analyzer),
            "generation_dependency_sha256": dependencies,
            "target": args.target,
            "platform": platform.platform(),
            "rustc": run([args.rustc, "--version"]).strip(),
            "rust_edition": args.edition,
            "commands": commands,
            "input_sha256": before_hashes,
            "inputs_unchanged": before_hashes
            == {tool: digest(path) for tool, path in paths.items()},
            "comparisons": comparisons,
            "record_name_mapping": pairs,
            "record_mapping_conflicts": conflicts,
            "field_name_mapping": field_renames,
            "excluded_storage_fields": excluded,
            "unsupported": {tool: api["unsupported"] for tool, api in apis.items()},
            "inventory": apis,
            "native_observations": probes,
            "limitations": [
                "This checks the generated Rust API and native Rust layout; independent C-compiled probes and real FFI calls remain required.",
                "Typedefs expand to target primitive types; usize/isize normalize to their 64-bit representation on the supported 64-bit target profiles.",
                "Record names match through shared typedefs and corresponding API type positions, bijectively; all resulting field shapes and native layouts are still compared.",
                "Generated private padding, alignment and bitfield storage fields are listed but their field shapes are excluded. Bitfield accessor semantics require C/Rust differential tests.",
                "Opaque records are compared as opaque types and excluded from native size/offset probes.",
                "Macro constant signedness, width, values and byte-string reference shapes are reported without coercing away differences.",
                "A successful comparison does not validate ABI register classification or every legal C input.",
            ],
        }
        report["equivalent"] = (
            bool(probes)
            and report["inputs_unchanged"]
            and not conflicts
            and not any(api["unsupported"] for api in apis.values())
            and all(matches(c) for c in comparisons.values())
            and not any(excluded.values())
        )
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        for key, comparison in comparisons.items():
            print(
                f"{key}: {comparison['equal']}/{comparison['common']} shared entries equal; {len(comparison['toucan_only'])} Toucan-only, {len(comparison['bindgen_only'])} bindgen-only"
            )
        print(f"report: {args.output}")
        return int(args.require_equivalent and not report["equivalent"])


if __name__ == "__main__":
    raise SystemExit(main())
