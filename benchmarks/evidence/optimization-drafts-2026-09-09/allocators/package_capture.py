#!/usr/bin/env python3
"""Package completed allocator evidence, or verify its hashes and math offline.

The verifier uses its hash-checked summarize.py sidecar and archived text inputs;
it never executes archived benchmark binaries or reads the original headers.
No builds, timing runs, Git operations, network access, or publication occur.
"""
import argparse
import gzip
import hashlib
import json
import re
import runpy
from pathlib import Path, PurePosixPath

FORMAT = "toucan-allocator-content-archive-v1"
NAMES = [f"{source}-{allocator}" for source in ("baseline", "optimized") for allocator in ("system", "jemalloc", "mimalloc")]
WORKLOADS = {(project, "toucan-builder") for project in ("zlib", "sqlite", "zstd", "libgit2")}
WORKLOADS |= {(project, "checked") for project in ("libgit2", "zlib", "zlib-adler32")}
SCRIPTS = ("build.py", "measure.py", "measure-reference.py", "summarize.py", "package_capture.py")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def json_bytes(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()


def safe_name(name):
    require(isinstance(name, str) and name, f"invalid archive path: {name!r}")
    path = PurePosixPath(name)
    require(not path.is_absolute() and ".." not in path.parts and str(path) == name, f"unsafe archive path: {name!r}")
    return name


class Capture:
    def __init__(self, work):
        self.work = work
        self.files = {}
        self.contents = {}
        self.originals = {}

    def add(self, path, name=None):
        path = Path(path)
        name = safe_name(name or path.relative_to(self.work).as_posix())
        require(path.is_file() and not path.is_symlink(), f"missing/nonregular evidence file: {path}")
        before = path.stat()
        data = path.read_bytes()
        after = path.stat()
        require((before.st_size, before.st_mtime_ns, before.st_ino) == (after.st_size, after.st_mtime_ns, after.st_ino), f"evidence changed while reading: {path}")
        text, sha = data.decode(), digest(data)
        require(name not in self.files or self.files[name] == sha, f"conflicting archive file: {name}")
        require(sha not in self.contents or self.contents[sha] == text, "SHA-256 content collision")
        require(path not in self.originals or self.originals[path] == sha, f"evidence changed: {path}")
        self.files[name], self.contents[sha], self.originals[path] = sha, text, sha
        return sha

    def read_json(self, path, name=None):
        self.add(path, name)
        return json.loads(path.read_bytes())


def stored_bytes(data, name):
    return data["contents"][data["files"][safe_name(name)]].encode()


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


def capture_files(capture, root, relative, prefix=""):
    path = root / relative
    data = capture.read_json(path, prefix + relative)
    require(data.get("status") == "passed", f"incomplete capture: {path}")
    folder = path.parent
    for row in data["rows"]:
        filename = f"{row['project']}-{row['engine']}-{row['variant']}-{row['round']}.out"
        output = folder / filename
        name = prefix + output.relative_to(root).as_posix()
        require(Path(row["command"][-1]).resolve() == output.resolve(), f"output command/path mismatch: {output}")
        require(capture.add(output, name) == row["output_sha256"], f"output changed: {output}")
        capture.add(output.with_suffix(".rss"), name.removesuffix(".out") + ".rss")
        if data["mode"] == "preflight" and row["engine"] == "toucan-builder":
            capture.add(output.with_suffix(".report.json"), name.removesuffix(".out") + ".report.json")
    return data


def verify_raw(data):
    records = stored_json(data, "variants.json")
    require([record["name"] for record in records] == NAMES, "incomplete/reordered allocator builds")
    variants = {record["name"]: record for record in records}
    baseline = stored_json(data, "primary/preflight/baseline/capture.json")
    require(baseline.get("status") == "passed" and baseline.get("mode") == "preflight", "primary baseline is incomplete")
    baseline_rows = {(row["project"], row["engine"]): row for row in baseline["rows"]}
    require(len(baseline["rows"]) == len(WORKLOADS) and set(baseline_rows) == WORKLOADS, "primary baseline workload coverage differs")
    require(data["checks"]["variants"] == NAMES and {tuple(item) for item in data["checks"]["workloads"]} == WORKLOADS, "stored variant/workload inventory differs")
    require(data["checks"]["primary_baseline_observations"] == len(WORKLOADS), "stored primary observation count differs")
    summary = stored_json(data, "summary.json")
    require(summary.get("status") == "passed", "summary is incomplete")
    rounds = summary["rounds"]
    require(rounds and rounds == list(range(len(rounds))), "invalid timing rounds")
    counts = {}
    for relative, mode, names, expected_rounds, original_root in (
        ("primary/preflight/baseline/capture.json", "preflight", ["baseline"], [0], data["roots"]["primary"]),
        ("preflight/all/capture.json", "preflight", NAMES, [0], data["roots"]["work"]),
        ("timing/all/capture.json", "timing", NAMES, rounds, data["roots"]["work"]),
    ):
        capture = stored_json(data, relative)
        require(capture.get("status") == "passed" and capture.get("mode") == mode, f"incomplete/wrong capture: {relative}")
        require([item["name"] for item in capture["variants"]] == names, f"capture variant order changed: {relative}")
        if names == NAMES:
            require(all(item == variants[item["name"]] for item in capture["variants"]), f"build records changed: {relative}")
        seen = set()
        for row in capture["rows"]:
            key = (row["project"], row["engine"], row["variant"], row["round"])
            require(key not in seen, f"duplicate observation: {relative}/{key}")
            seen.add(key)
            require(not row["instrumented"] and all(value is None for value in row["allocations"]), "allocator capture contains System counters")
            filename = f"{key[0]}-{key[1]}-{key[2]}-{key[3]}.out"
            name = str(PurePosixPath(relative).parent / filename)
            expected = baseline_rows[key[:2]]
            require(data["files"][name] == row["output_sha256"] == expected["output_sha256"], f"output differs from baseline: {name}")
            require(int(stored_bytes(data, name.removesuffix(".out") + ".rss")) == row["peak_rss_kib"], f"RSS data differs: {name}")
            require(row["configuration"] == expected["configuration"], f"configuration changed: {name}")
            command = row["command"]
            original_folder = str(PurePosixPath(relative.removeprefix("primary/")).parent)
            require(len(command) == 5 and command[1] == key[1] and int(command[3]) == len(row["samples_ms"]), f"invocation differs from samples: {name}")
            require(command[-1] == original_root.rstrip("/") + "/" + original_folder + "/" + filename, f"invocation output differs: {name}")
            if names == NAMES:
                require(command[0] == variants[key[2]]["binary"], f"wrong executable: {name}")
            request_name = data["request_paths"][command[2]]
            require(data["files"][request_name] == data["external_inputs"][command[2]]["sha256"], f"request hash differs: {request_name}")
            if mode == "preflight" and key[1] == "toucan-builder":
                report = stored_json(data, name.removesuffix(".out") + ".report.json")
                golden = f"primary/preflight/baseline/{key[0]}-toucan-builder-baseline-0.report.json"
                require("timings" not in report and report == stored_json(data, golden), f"normalized report changed: {name}")
        require(seen == {(project, engine, name, number) for project, engine in WORKLOADS for name in names for number in expected_rounds}, f"observation coverage differs: {relative}")
        counts[relative] = len(seen)
    timing = stored_json(data, "timing/all/capture.json")
    require(set(timing["inputs"]) == set(data["external_inputs"]), "external input inventory differs")
    for name, sha in timing["inputs"].items():
        require(data["external_inputs"][name]["sha256"] == sha, f"external input hash differs: {name}")
    require(set(data["external_binaries"]) == {record["binary"] for record in records}, "external binary inventory differs")
    for record in records:
        name = record["name"]
        require(stored_json(data, f"{name}-build.json") == record, f"build receipt differs: {name}")
        require(data["external_binaries"][record["binary"]]["sha256"] == record["sha256"], f"executable hash differs: {name}")
        require(f"build-{name}.log" in data["files"], f"missing build log: {name}")
    require(data["checks"]["timing_observations"] == counts["timing/all/capture.json"] and data["checks"]["preflight_observations"] == counts["preflight/all/capture.json"], "stored observation count differs")


def verify_math(data, summarizer):
    # This is the verifier's explicit, hash-checked helper sidecar, never code
    # extracted from an archive content block or selected by an archive path.
    require(file_digest(summarizer) == data["files"]["summarize.py"], "summarizer sidecar differs from archive")
    module = runpy.run_path(str(summarizer))
    work = ArchivePath(data, "", data["roots"]["work"])
    primary = ArchivePath(data, "primary", data["roots"]["primary"])
    calculated = module["summarize"](work / "timing/all/capture.json", work / "preflight/all/capture.json", work / "build-inputs", primary)
    require(calculated == stored_json(data, "summary.json"), "summary arithmetic, pairing, coverage, or provenance differs from raw captures")
    require((module["markdown"](calculated) + "\n").encode() == stored_bytes(data, "summary.md"), "Markdown summary differs from calculated results")


def verify_data(compressed, manifest):
    require(digest(compressed) == manifest["archive"]["sha256"] and len(compressed) == manifest["archive"]["bytes"], "compressed archive hash/size mismatch")
    data = json.loads(gzip.decompress(compressed))
    require(data["format"] == manifest["format"] == FORMAT, "unknown archive format")
    require(data["files"] == manifest["files"] and data["checks"]["status"] == "passed", "file manifest/status differs")
    for sha, text in data["contents"].items():
        require(isinstance(text, str) and re.fullmatch(r"[0-9a-f]{64}", sha) and digest(text.encode()) == sha, f"invalid stored content: {sha}")
    for name, sha in data["files"].items():
        safe_name(name)
        require(sha in data["contents"], f"missing stored content: {name}")
    require(set(data["contents"]) == set(data["files"].values()), "unreferenced stored content")
    require(len(data["files"]) == manifest["file_count"] and len(data["contents"]) == manifest["unique_content_count"], "manifest file/content count mismatch")
    require(sum(len(text.encode()) for text in data["contents"].values()) == manifest["unique_utf8_bytes"], "unique byte count differs")
    require(sum(len(stored_bytes(data, name)) for name in data["files"]) == manifest["logical_utf8_bytes"], "logical byte count differs")
    for kind in ("inputs", "binaries"):
        require(len(data["external_" + kind]) == manifest["external_" + kind + "_count"], f"external {kind} count differs")
        for name, item in data["external_" + kind].items():
            require(re.fullmatch(r"[0-9a-f]{64}", item["sha256"]) and isinstance(item["bytes"], int) and item["bytes"] >= 0, f"invalid external file metadata: {name}")
    require(set(manifest["sidecars"]) == {"README.md", "summary.json", "summary.md", "package_capture.py", "summarize.py"}, "missing/unexpected artifact sidecars")
    for name, sha in manifest["sidecars"].items():
        require(data["files"].get(safe_name(name)) == sha, f"sidecar differs from archive: {name}")
    verify_raw(data)
    return data


def verify(archive, manifest_path):
    manifest = json.loads(manifest_path.read_bytes())
    data = verify_data(archive.read_bytes(), manifest)
    for name, sha in manifest["sidecars"].items():
        require(file_digest(manifest_path.parent / safe_name(name)) == sha, f"sidecar hash differs: {name}")
    verify_math(data, manifest_path.parent / "summarize.py")
    return manifest


def package(work, primary, output, readme):
    require(not output.exists(), f"destination already exists: {output}")
    capture = Capture(work)
    summary = capture.read_json(work / "summary.json")
    require(summary.get("status") == "passed", "summary has not passed")
    capture.add(work / "summary.md")
    records = capture.read_json(work / "variants.json")
    require([record["name"] for record in records] == NAMES, "all six allocator builds are required")
    baseline = capture_files(capture, primary, "preflight/baseline/capture.json", "primary/")
    preflight = capture_files(capture, work, "preflight/all/capture.json")
    timing = capture_files(capture, work, "timing/all/capture.json")
    capture.add(primary / "variants.json", "primary/variants.json")
    capture.add(primary / "host.json", "primary/host.json")
    for path in sorted((primary / "build-inputs/baseline").iterdir()):
        capture.add(path, "primary/build-inputs/baseline/" + path.name)
    external_inputs = {}
    for name, sha in timing["inputs"].items():
        path = Path(name)
        require(file_digest(path) == sha, f"input changed: {name}")
        external_inputs[name] = {"sha256": sha, "bytes": path.stat().st_size}
    request_paths = {}
    for request in sorted({Path(row["command"][2]) for row in timing["rows"]}):
        if request.is_relative_to(primary / "checked-sources"):
            name = "primary/checked-sources/" + request.name
            project = request.name.removesuffix(".request.json")
            for suffix in (".provenance.json", ".snapshot.json", ".run.json"):
                path = request.with_name(project + suffix)
                capture.add(path, "primary/checked-sources/" + path.name)
        else:
            name = "prior/" + request.name
            reference = request.with_name(request.name.replace(".request.json", ".reference.json"))
            capture.add(reference, "prior/" + reference.name)
        capture.add(request, name)
        request_paths[str(request)] = name
    external_binaries = {}
    for record in records:
        name = record["name"]
        capture.add(work / f"{name}-build.json")
        capture.add(work / f"build-{name}.log")
        require(len(record["builds"]) == 1, f"unexpected build count: {name}")
        frozen = work / "build-inputs" / name
        require({path.name for path in frozen.iterdir()} == set(record["builds"][0]["driver_files"]), f"frozen file set changed: {name}")
        for filename, sha in record["builds"][0]["driver_files"].items():
            require(capture.add(frozen / filename) == sha, f"frozen build input changed: {name}/{filename}")
        binary = Path(record["binary"])
        require(file_digest(binary) == record["sha256"], f"frozen binary changed: {name}")
        external_binaries[str(binary)] = {"sha256": record["sha256"], "bytes": binary.stat().st_size}
    for name in (*SCRIPTS, "helper-source.json", "measurement-source.json", "frontend-source-equivalence.json", "driver-template.toml"):
        capture.add(work / name)
    for name in ("allocator-source-comparison.json", "host-build-context.json", "pre-timing-validation.json", "feature-checks.json", "check_features.py"):
        capture.add(work / name)
    for name in ("summary-audit.json", "summary-retry.json"):
        capture.add(work / name)
    for path in sorted((work / "logs").glob("*")):
        if path.is_file() and path.suffix in (".log", ".json"):
            capture.add(path)
    for path in sorted((work / "pr-source").glob("*.rs")):
        capture.add(path)
    for directory in ("driver/src", "reference-driver"):
        for path in sorted((work / directory).glob("*.rs")):
            capture.add(path)
    for name in ("Cargo.toml", "Cargo.lock"):
        capture.add(work / "driver" / name)
    for path in sorted(work.glob("*.log")):
        capture.add(path)
    capture.add(work / "README.md", "preparation/README.md")
    capture.add(readme, "README.md")
    for path, sha in capture.originals.items():
        require(file_digest(path) == sha, f"evidence changed during packaging: {path}")
    for name, metadata in {**external_inputs, **external_binaries}.items():
        require(file_digest(Path(name)) == metadata["sha256"], f"external file changed during packaging: {name}")
    archive = {"format": FORMAT, "files": capture.files, "contents": capture.contents,
               "roots": {"work": str(work), "primary": str(primary)}, "request_paths": request_paths,
               "external_inputs": external_inputs, "external_binaries": external_binaries,
               "scope": "Text helpers, locks, raw captures, logs, normalized reports, outputs and primary baseline goldens. Physical input files and executables are hash/size inventories only, except embedded request JSON. Checked output snapshots contain their recorded preprocessed source.",
               "checks": {"status": "passed", "variants": NAMES, "workloads": sorted(WORKLOADS), "primary_baseline_observations": len(baseline["rows"]), "preflight_observations": len(preflight["rows"]), "timing_observations": len(timing["rows"])}}
    compressed = gzip.compress(json.dumps(archive, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode(), compresslevel=9, mtime=0)
    sidecars = ("README.md", "summary.json", "summary.md", "package_capture.py", "summarize.py")
    manifest = {"format": FORMAT, "archive": {"path": "capture.json.gz", "sha256": digest(compressed), "bytes": len(compressed)}, "files": capture.files,
                "file_count": len(capture.files), "unique_content_count": len(capture.contents),
                "unique_utf8_bytes": sum(len(text.encode()) for text in capture.contents.values()),
                "logical_utf8_bytes": sum(len(stored_bytes(archive, name)) for name in capture.files),
                "external_inputs_count": len(external_inputs), "external_binaries_count": len(external_binaries),
                "sidecars": {name: capture.files[name] for name in sidecars}}
    verify_data(compressed, manifest)
    verify_math(archive, work / "summarize.py")
    output.mkdir(parents=True, exist_ok=False)
    (output / "capture.json.gz").write_bytes(compressed)
    (output / "manifest.json").write_bytes(json_bytes(manifest))
    for name in sidecars:
        (output / name).write_bytes(stored_bytes(archive, name))
    verify(output / "capture.json.gz", output / "manifest.json")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    pack = commands.add_parser("package")
    pack.add_argument("--work", type=Path, default=Path(__file__).resolve().parent)
    pack.add_argument("--primary", type=Path)
    pack.add_argument("--output", type=Path)
    pack.add_argument("--readme", type=Path)
    check = commands.add_parser("verify")
    check.add_argument("archive", type=Path)
    check.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "package":
            work = args.work.resolve()
            result = package(work, (args.primary or work.parent).resolve(), (args.output or work / "packaged").resolve(), (args.readme or work / "README-draft.md").resolve())
        else:
            result = verify(args.archive, args.manifest or args.archive.with_name("manifest.json"))
        print(json.dumps({"status": "passed", "archive_sha256": result["archive"]["sha256"], "files": result["file_count"], "unique_contents": result["unique_content_count"], "compressed_bytes": result["archive"]["bytes"]}))
    except (ValueError, KeyError, TypeError, OSError, UnicodeError, EOFError, IndexError) as error:
        parser.exit(2, f"Cannot {args.command} allocator evidence: {error}\n")


if __name__ == "__main__":
    main()
