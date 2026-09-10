#!/usr/bin/env python3
"""Package completed optimization evidence, or verify an existing package offline.

No builds, measurements, Git operations, network requests, or publication occur.
Run `package` only after timing/allocation captures and summary.json have passed.
The default destination is a new `packaged` directory beside this script.
"""
import argparse
import gzip
import hashlib
import json
import re
import runpy
import tomllib
import sys
from pathlib import Path, PurePosixPath

FORMAT = "toucan-selected-optimization-content-archive-v1"
SCRIPTS = ("build_variant.py", "measure.py", "summarize.py", "package_capture.py", "run_captures.py", "capture_host.py")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def safe_name(name):
    path = PurePosixPath(name)
    require(isinstance(name, str) and name and not path.is_absolute()
            and ".." not in path.parts and str(path) == name, f"unsafe archive path: {name!r}")
    return name


class Capture:
    def __init__(self, work):
        self.work = work
        self.files = {}
        self.contents = {}
        self.originals = {}
        self.workloads = set()

    def add_text(self, name, data):
        safe_name(name)
        text = data.decode("utf-8")
        sha = digest(data)
        require(name not in self.files or self.files[name] == sha, f"conflicting archive file: {name}")
        require(sha not in self.contents or self.contents[sha] == text, "SHA-256 content collision")
        self.files[name] = sha
        self.contents[sha] = text
        return sha

    def add(self, path, name=None):
        path = Path(path)
        name = name or path.relative_to(self.work).as_posix()
        require(path.is_file() and not path.is_symlink(), f"missing/nonregular evidence file: {path}")
        before = path.stat()
        data = path.read_bytes()
        after = path.stat()
        require((before.st_size, before.st_mtime_ns, before.st_ino)
                == (after.st_size, after.st_mtime_ns, after.st_ino), f"file changed while reading: {path}")
        sha = self.add_text(name, data)
        require(path not in self.originals or self.originals[path] == sha, f"file changed: {path}")
        self.originals[path] = sha
        return sha

    def read_json(self, relative):
        self.add(self.work / relative)
        return json.loads(self.contents[self.files[relative]])


def check_capture(capture, relative, mode, expected_names, variants, rounds, baseline_outputs=None):
    data = capture.read_json(relative)
    require(data.get("status") == "passed" and data.get("mode") == mode, f"incomplete/wrong capture: {relative}")
    require([item["name"] for item in data["variants"]] == expected_names, f"variant order changed: {relative}")
    for variant in data["variants"]:
        require(variant == variants[variant["name"]], f"build records changed: {relative}/{variant['name']}")
    folder = PurePosixPath(relative).parent
    observed = set()
    for row in data["rows"]:
        key = (row["project"], row["engine"], row["variant"], row["round"])
        require(key not in observed, f"duplicate observation: {relative}/{key}")
        observed.add(key)
        require(row["instrumented"] == (mode == "allocations"), f"wrong instrumentation: {relative}/{key}")
        output_name = str(folder / f"{key[0]}-{key[1]}-{key[2]}-{key[3]}.out")
        output_path = capture.work / output_name
        require(Path(row["command"][-1]).resolve() == output_path.resolve(), f"output command/path mismatch: {key}")
        sha = capture.add(output_path)
        require(sha == row["output_sha256"], f"output hash changed: {output_name}")
        if baseline_outputs is not None:
            require(sha == baseline_outputs[key[:2]], f"output differs from baseline: {output_name}")
        rss_path = output_path.with_suffix(".rss")
        capture.add(rss_path)
        require(int(rss_path.read_text()) == row["peak_rss_kib"], f"RSS record changed: {rss_path}")
        if mode == "preflight" and key[1] == "toucan-builder":
            report_path = output_path.with_suffix(".report.json")
            capture.add(report_path)
            report = json.loads(report_path.read_bytes())
            baseline_path = capture.work / "preflight/baseline" / f"{key[0]}-toucan-builder-baseline-0.report.json"
            require(report == json.loads(baseline_path.read_bytes()), f"report differs from baseline: {report_path}")
            require("timings" not in report, f"report retains timing fields: {report_path}")
    expected = {(project, engine, name, round_number) for project, engine in capture.workloads
                for name in expected_names for round_number in rounds}
    require(observed == expected, f"missing/extra process observations: {relative}")
    for path in sorted((capture.work / str(folder)).rglob("*")):
        if path.is_file() and path.suffix in (".out", ".json", ".rss"):
            capture.add(path)
    return data


def package(work, output, readme):
    require(not output.exists(), f"destination already exists: {output}")
    capture = Capture(work)
    summary = capture.read_json("summary.json")
    require(summary.get("status") == "passed", "summary.json has not passed")
    timing_preview = capture.read_json("timing/all/capture.json")
    require(timing_preview.get("status") == "passed", "timing capture has not passed")
    capture.workloads = {(row["project"], row["engine"]) for row in timing_preview["rows"]}
    require(capture.workloads and capture.workloads
            == {(row["project"], row["engine"]) for row in summary["workloads"]}, "summary/workload set differs")
    drafts = capture.read_json("drafts.json")
    records = capture.read_json("variants.json")
    names = ["baseline", *[item["name"] for item in drafts]]
    require(len(names) == len(set(names)) and [item["name"] for item in records] == names, "incomplete/reordered variant builds")
    variants = {item["name"]: item for item in records}
    for draft in drafts:
        require(variants[draft["name"]]["head"] == draft["head"], f"draft revision changed: {draft['name']}")
    for path in ("timing/all/capture.json", "allocations/all/capture.json", "drafts.json", "canonical-inputs.json"):
        require(summary["sources"].get(str(work / path)) == file_digest(work / path), f"summary is stale: {path}")
    for mode in ("timing", "allocation"):
        rounds = summary[f"{mode}_rounds"]
        require(rounds and rounds == list(range(len(rounds))), f"invalid {mode} round list")
    baseline = check_capture(capture, "preflight/baseline/capture.json", "preflight", ["baseline"], variants, [0])
    baseline_outputs = {(row["project"], row["engine"]): row["output_sha256"] for row in baseline["rows"]}
    preflights = [baseline]
    for name in names[1:]:
        preflights.append(check_capture(capture, f"preflight/{name}/capture.json", "preflight", [name], variants, [0], baseline_outputs))
    timing = check_capture(capture, "timing/all/capture.json", "timing", names, variants, summary["timing_rounds"], baseline_outputs)
    allocation = check_capture(capture, "allocations/all/capture.json", "allocations", names, variants, summary["allocation_rounds"], baseline_outputs)
    require(timing["inputs"] == allocation["inputs"], "timing/allocation input hashes differ")
    for data in preflights:
        require(timing["inputs"] == data["inputs"], "preflight input hashes differ")
    external_inputs = {}
    for name, sha in sorted(timing["inputs"].items()):
        path = Path(name)
        require(file_digest(path) == sha, f"upstream input changed: {path}")
        external_inputs[name] = {"sha256": sha, "bytes": path.stat().st_size}
    # Requests and their historical reference metadata are small reproducibility
    # inputs. Original upstream header contents remain outside this archive.
    requests = {Path(row["command"][2]) for row in timing["rows"]}
    for request in sorted(requests):
        if request.name == "zlib-adler32.request.json":
            project = request.name.removesuffix(".request.json")
            capture.add(request, "historical-checked-source/" + request.name)
            provenance_path = request.with_name(project + ".provenance.json")
            capture.add(provenance_path, "historical-checked-source/" + provenance_path.name)
            provenance = json.loads(provenance_path.read_bytes())
            require(file_digest(request) == provenance["request_sha256"], f"checked-source request changed: {request}")
            require(timing["inputs"].get(provenance["source"]) == provenance["source_sha256"], f"checked source changed: {project}")
            require(all(timing["inputs"].get(path) == sha for path, sha in provenance["dependency_sha256"].items()), f"checked-source dependencies changed: {project}")
            snapshot = request.with_name(project + ".snapshot.json")
            require(capture.add(snapshot, "historical-checked-source/" + snapshot.name) == provenance["snapshot_sha256"], f"historical checked-source snapshot changed: {project}")
            capture.add(request.with_name(project + ".run.json"), "historical-checked-source/" + project + ".run.json")
            continue
        capture.add(request, "prior/" + request.name)
        reference = request.with_name(request.name.replace(".request.json", ".reference.json"))
        capture.add(reference, "prior/" + reference.name)
        metadata = json.loads(reference.read_bytes())
        require(json.loads(request.read_bytes()) == metadata["request"], f"prior request changed: {request}")
        project = request.name.split(".")[0]
        require({row["output_sha256"] for row in metadata["observations"]["toucan-builder"]}
                == {baseline_outputs[(project, "toucan-builder")]}, f"prior Builder output differs: {project}")
    external_binaries = {}
    for name in names:
        record = variants[name]
        require(capture.read_json(f"{name}-build.json") == record, f"variant build receipt changed: {name}")
        require(len(record["builds"]) == 2, f"missing timing/allocation build: {name}")
        for counted, build in enumerate(record["builds"]):
            suffix = "-allocations" if counted else ""
            frozen = work / "build-inputs" / (name + suffix)
            require({path.name for path in frozen.iterdir()} == set(build["driver_files"]), f"build-input file set changed: {frozen}")
            for filename, sha in build["driver_files"].items():
                require(capture.add(frozen / filename) == sha, f"frozen driver changed: {name}{suffix}/{filename}")
            capture.add(work / f"build-{name}{suffix}.log")
            binary = Path(record["alloc_binary" if counted else "binary"])
            sha = record["alloc_sha256" if counted else "sha256"]
            require(file_digest(binary) == sha, f"frozen binary changed: {binary}")
            external_binaries[str(binary)] = {"sha256": sha, "bytes": binary.stat().st_size}
    for script in SCRIPTS:
        capture.add(work / script)
    for path in sorted((work / "driver").rglob("*")):
        if path.is_file() and (path.suffix == ".rs" or path.name in ("Cargo.toml", "Cargo.lock")):
            require("target" not in path.relative_to(work / "driver").parts, "unexpected driver build artifact")
            capture.add(path)
    for name in ("selection.json", "dependency-policy.json", "preparation.json", "canonical-inputs.json", "native-idle.json", "host.json", "capture-execution.json"):
        capture.add(work / name)
    for name in ("summary.md", "build-stack.log", "preflight.log", "timing.log", "allocations.log", "disk-incident.json", "method-audit.json", "host.json", "capture-execution.json", "result-audit.json", "allocator-draft.json"):
        if (work / name).is_file():
            capture.add(work / name)
    for folder in ("historical-baseline", "historical-timing", "reference-driver", "locks", "logs", "attempts"):
        for path in sorted((work / folder).rglob("*")):
            if path.is_file():
                capture.add(path)
    for path in sorted(work.glob("*.log")):
        capture.add(path)
    for name in ("validation.json", "helper-validation.json", "result-audit.json", "historical-host.json"):
        if (work / name).is_file():
            capture.add(work / name)
    capture.add(readme, "README.md")
    # Reject concurrent writes rather than blessing a mixture of capture versions.
    for path, sha in capture.originals.items():
        require(file_digest(path) == sha, f"evidence changed while packaging: {path}")
    for name, metadata in external_inputs.items():
        require(file_digest(Path(name)) == metadata["sha256"], f"input changed while packaging: {name}")
    archive = {"format": FORMAT, "files": capture.files, "contents": capture.contents, "original_root": str(work),
               "external_inputs": external_inputs, "external_binaries": external_binaries,
               "scope": "UTF-8 helper source and raw evidence; original upstream headers and executable binaries are identified by SHA-256, not embedded. Checked output snapshots include their recorded preprocessed source.",
               "checks": {"status": "passed", "variants": names,
                          "timing_observations": len(timing["rows"]), "allocation_observations": len(allocation["rows"])}}
    raw = json.dumps(archive, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    compressed = gzip.compress(raw, compresslevel=9, mtime=0)
    sidecars = {name: capture.contents[capture.files[name]].encode("utf-8")
                for name in ("README.md", "summary.json", "summary.md", "package_capture.py", "summarize.py") if name in capture.files}
    manifest = {"format": FORMAT, "archive": {"path": "capture.json.gz", "sha256": digest(compressed), "bytes": len(compressed)},
                "files": capture.files, "file_count": len(capture.files), "unique_content_count": len(capture.contents),
                "unique_utf8_bytes": sum(len(text.encode("utf-8")) for text in capture.contents.values()),
                "logical_utf8_bytes": sum(len(capture.contents[sha].encode("utf-8")) for sha in capture.files.values()),
                "sidecars": {name: digest(data) for name, data in sidecars.items()},
                "external_input_count": len(external_inputs), "external_binary_count": len(external_binaries)}
    # In-memory verification precedes any artifact writes. A fresh output directory
    # prevents accidental replacement of previous captures or repository files.
    verified = verify_data(compressed, manifest)
    verify_math(verified, work / "summarize.py")
    output.mkdir(parents=True, exist_ok=False)
    (output / "capture.json.gz").write_bytes(compressed)
    (output / "manifest.json").write_bytes(json_bytes(manifest))
    for name, data in sidecars.items():
        (output / name).write_bytes(data)
    verify(output / "capture.json.gz", output / "manifest.json")
    return manifest


def verify_data(compressed, manifest):
    require(digest(compressed) == manifest["archive"]["sha256"], "compressed archive SHA-256 mismatch")
    require(len(compressed) == manifest["archive"]["bytes"], "compressed archive size mismatch")
    data = json.loads(gzip.decompress(compressed))
    require(data["format"] == manifest["format"] == FORMAT, "unknown archive format")
    require(data["files"] == manifest["files"], "file manifest differs from archive")
    require(data["checks"]["status"] == "passed", "archive was not packaged from passed captures")
    contents = data["contents"]
    for sha, text in contents.items():
        require(isinstance(text, str) and re.fullmatch(r"[0-9a-f]{64}", sha), "invalid content record")
        require(digest(text.encode("utf-8")) == sha, f"stored content SHA-256 mismatch: {sha}")
    for name, sha in data["files"].items():
        safe_name(name)
        require(sha in contents, f"missing content for {name}")
    require(set(contents) == set(data["files"].values()), "unreferenced stored contents")
    require(len(data["files"]) == manifest["file_count"] and len(contents) == manifest["unique_content_count"], "manifest count mismatch")
    require(sum(len(text.encode("utf-8")) for text in contents.values()) == manifest["unique_utf8_bytes"], "unique content byte count mismatch")
    require(sum(len(contents[sha].encode("utf-8")) for sha in data["files"].values()) == manifest["logical_utf8_bytes"], "logical byte count mismatch")
    require(len(data["external_inputs"]) == manifest["external_input_count"] and len(data["external_binaries"]) == manifest["external_binary_count"], "external file count mismatch")
    for name, sha in manifest["sidecars"].items():
        require(data["files"].get(name) == sha, f"sidecar disagrees with archive: {name}")
    return data


def stored_bytes(data, name):
    return data["contents"][data["files"][safe_name(name)]].encode("utf-8")


def stored_json(data, name):
    return json.loads(stored_bytes(data, name))


class ArchivePath:
    """Read-only path adapter for the existing summarizer, with no extraction."""
    def __init__(self, data, logical, original):
        self.data, self.logical, self.original = data, logical, original

    def __truediv__(self, suffix):
        return ArchivePath(self.data, safe_name(self.logical + "/" + suffix) if self.logical else safe_name(suffix), self.original.rstrip("/") + "/" + suffix)

    @property
    def parent(self):
        logical = str(PurePosixPath(self.logical).parent)
        return ArchivePath(self.data, "" if logical == "." else logical, self.original.rsplit("/", 1)[0])

    def __str__(self):
        return self.original

    def read_bytes(self):
        return stored_bytes(self.data, self.logical)

    def read_text(self):
        return self.read_bytes().decode()


def verify_math(data, summarizer):
    require(file_digest(summarizer) == data["files"]["summarize.py"], "summarizer sidecar hash mismatch")
    module = runpy.run_path(str(summarizer))
    work = ArchivePath(data, "", data["original_root"])
    inventory, inventory_sha = module["validate_inventory"](work)
    calculated = module["summarize"](work / "timing/all/capture.json", work / "allocations/all/capture.json", work / "drafts.json")
    require(calculated == stored_json(data, "summary.json"), "summary math or provenance differs from captures")
    require((module["markdown"](calculated) + "\n").encode() == stored_bytes(data, "summary.md"), "Markdown summary differs from calculated results")
    names = ["baseline", "selected"]
    workloads = {(item["project"], item["engine"]) for item in calculated["workloads"]}
    goldens = {key: data["files"][f"preflight/baseline/{key[0]}-{key[1]}-baseline-0.out"] for key in workloads}
    for relative in ("preflight/baseline/capture.json", "preflight/selected/capture.json", "timing/all/capture.json", "allocations/all/capture.json"):
        capture = stored_json(data, relative)
        require(capture["status"] == "passed", "incomplete archived capture")
        require(capture["inputs"] == inventory["inputs"] and capture["canonical_input_metadata_sha256"] == inventory_sha, f"archived canonical inventory differs: {relative}")
        expected_names = [relative.split("/")[1]] if relative.startswith("preflight/") else names
        expected_rounds = [0] if relative.startswith("preflight/") else calculated["allocation_rounds" if relative.startswith("allocations/") else "timing_rounds"]
        require([item["name"] for item in capture["variants"]] == expected_names, f"archived capture variants differ: {relative}")
        seen = set()
        for row in capture["rows"]:
            key = (row["project"], row["engine"], row["variant"], row["round"])
            require(key not in seen, f"duplicate archived process observation: {key}")
            seen.add(key)
            filename = f"{row['project']}-{row['engine']}-{row['variant']}-{row['round']}.out"
            name = str(PurePosixPath(relative).parent / filename)
            require(data["files"][name] == row["output_sha256"] == goldens[(row["project"], row["engine"])], f"archived output differs: {name}")
            require(int(stored_bytes(data, name.removesuffix(".out") + ".rss")) == row["peak_rss_kib"], f"archived RSS differs: {name}")
            historical = data["files"][f"historical-baseline/{row['project']}-{row['engine']}-baseline-0.out"]
            require(row["historical_output_sha256"] == historical and row["historical_output_equal"] == (historical == row["output_sha256"]), f"historical comparison differs: {name}")
            if relative.startswith("preflight/") and row["engine"] == "toucan-builder":
                report = stored_json(data, name.removesuffix(".out") + ".report.json")
                golden = stored_json(data, f"preflight/baseline/{row['project']}-toucan-builder-baseline-0.report.json")
                require(report == golden and "timings" not in report, f"archived report differs: {name}")
        require(seen == {(project, engine, variant, number) for project, engine in workloads for variant in expected_names for number in expected_rounds}, f"archived process coverage differs: {relative}")


def verify(archive_path, manifest_path):
    manifest = json.loads(manifest_path.read_bytes())
    verified = verify_data(archive_path.read_bytes(), manifest)
    for name, sha in manifest["sidecars"].items():
        require(file_digest(manifest_path.parent / safe_name(name)) == sha, f"sidecar SHA-256 mismatch: {name}")
    verify_math(verified, manifest_path.parent / "summarize.py")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    pack = commands.add_parser("package", help="require completed captures, then create a new artifact directory")
    pack.add_argument("--work", type=Path, default=Path(__file__).resolve().parent)
    pack.add_argument("--output", type=Path)
    pack.add_argument("--readme", type=Path)
    check = commands.add_parser("verify", help="verify stored content hashes offline; external headers/binaries are not required")
    check.add_argument("archive", type=Path)
    check.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "package":
            work = args.work.resolve()
            result = package(work, (args.output or work / "packaged").resolve(), (args.readme or work / "README-draft.md").resolve())
        else:
            result = verify(args.archive, args.manifest or args.archive.with_name("manifest.json"))
        print(json.dumps({"status": "passed", "archive_sha256": result["archive"]["sha256"],
                          "files": result["file_count"], "unique_contents": result["unique_content_count"],
                          "compressed_bytes": result["archive"]["bytes"]}))
    except (ValueError, KeyError, TypeError, OSError, UnicodeError, EOFError) as error:
        parser.exit(2, f"Cannot {args.command} evidence: {error}\n")


if __name__ == "__main__":
    main()
