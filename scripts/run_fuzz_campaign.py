#!/usr/bin/env python3
"""Run a bounded ASan campaign and preserve the inputs needed to reproduce it."""

import argparse
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import tarfile
import time
from pathlib import Path


def sha256(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def capture(command, root):
    return subprocess.check_output(command, cwd=root, text=True).strip()


def source_manifest(root):
    names = capture(["git", "ls-files", "-z"], root).split("\0")
    return {name: sha256(root / name) for name in names if name}


def corpus_manifest(corpus):
    result = {}
    for path in sorted(corpus.iterdir()):
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"corpus entries must be regular files: {path}")
        result[path.name] = {"sha256": sha256(path), "bytes": path.stat().st_size}
    return result


def seed_profiles(data, profiles, modes=2):
    """Keep source bytes intact while selecting each profile in two, four, or eight modes."""
    if modes not in (2, 4, 8):
        raise ValueError("expected two, four, or eight language modes")
    prefix, suffix = data + b"\n/* profile ", b" */\n"
    total = sum(prefix) + sum(suffix)
    count = profiles or 1
    for mode in range(modes):
        for profile in range(count):
            # Overlapping ASCII ranges cover the 512-, 1024-, or 2048-value mode period.
            # Sixty-four padding bytes suffice for all supported profile counts.
            padding = next(
                b" " * spaces + bytes([byte])
                for spaces in range(8 * modes)
                for byte in range(33, 127)
                if byte not in (42, 47)
                and (total + 32 * spaces + byte) % count == profile
                and ((total + 32 * spaces + byte) >> 8) % modes == mode
            )
            yield prefix + padding + suffix


def archive_initial_corpus(corpus, output):
    manifest = corpus_manifest(corpus)
    with tarfile.open(output / "initial-corpus.tar.gz", "w:gz") as archive:
        for name in manifest:
            archive.add(corpus / name, arcname=name, recursive=False)
    return manifest


def seed_preprocessor_policies(data):
    """Select all 80 comment/query/trigraph/scope/macro-history settings."""
    prefix, suffix = data + b"\n/* profile ", b" */\n"
    total = sum(prefix) + sum(suffix)
    for comments in range(5):
        for trigraphs in (False, True):
            for dialect in range(2):
                for scope in (False, True):
                    for history in (False, True):
                        # Five comment policies repeat after 2560 checksum values.
                        # Cover that period without introducing a comment end.
                        padding = next(
                            b" " * spaces + bytes([byte])
                            for spaces in range(81)
                            for byte in range(33, 127)
                            if byte not in (42, 47)
                            and (total + 32 * spaces + byte) & 1 == dialect
                            and bool((total + 32 * spaces + byte) & 2) == scope
                            and bool((total + 32 * spaces + byte) & 4) == history
                            and bool((total + 32 * spaces + byte) & 0x100) == trigraphs
                            and ((total + 32 * spaces + byte) >> 9) % 5 == comments
                        )
                        yield prefix + padding + suffix


def run_fuzzer(command, root, output, seconds):
    started = time.monotonic()
    with (output / "fuzz.log").open("wb") as log:
        try:
            result = subprocess.run(
                command,
                cwd=root,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=seconds + 60,
                check=False,
            )
            exit_code, timed_out = result.returncode, False
        except subprocess.TimeoutExpired:
            exit_code, timed_out = None, True
    log_text = (output / "fuzz.log").read_text(errors="replace")
    statistics = {
        name: int(value)
        for name, value in re.findall(r"stat::(\w+):\s+(\d+)", log_text)
    }
    return {
        "exit_code": exit_code,
        "wall_timeout": timed_out,
        "elapsed_seconds": time.monotonic() - started,
        "statistics": statistics,
        "log_sha256": sha256(output / "fuzz.log"),
    }


def campaign_passed(report):
    return (
        report["exit_code"] == 0
        and not report["wall_timeout"]
        and not report["artifacts"]
        and report["source_unchanged"]
        and report["statistics"].get("number_of_executed_units", 0) > 0
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "target", choices=["preprocess", "semantic", "bindings", "checked"]
    )
    parser.add_argument("--seconds", type=int, default=900)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--profiles", type=int, default=11)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.seconds <= 3600 or not 1 <= args.profiles <= 32:
        parser.error("seconds must be 1..3600 and profiles must be 1..32")
    if not 1 <= args.seed < 2**32:
        parser.error("seed must be a nonzero unsigned 32-bit integer")
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = source_manifest(root)
    (output / "source.json").write_text(json.dumps(source, indent=2) + "\n")
    report = {
        "schema_version": 1,
        "source_commit": capture(["git", "rev-parse", "HEAD"], root),
        "source_status": capture(["git", "status", "--porcelain"], root),
        "source_manifest_sha256": sha256(output / "source.json"),
        "runner_sha256": sha256(Path(__file__)),
        "target": args.target,
        "profiles": None if args.target == "preprocess" else args.profiles,
        "target_selector": None
        if args.target == "preprocess"
        else "sum(input bytes) % profiles",
        "language_mode_selector": None
        if args.target == "preprocess"
        else "(sum(input bytes) >> 8) & 7: 0=gnu11, 1=c11, 2=gnu90, 3=c90, 4=gnu99, 5=c99, 6=gnu17, 7=c17",
        "language_mode_selector_version": None if args.target == "preprocess" else 3,
        "binding_derive_selector_version": 1 if args.target == "bindings" else None,
        "binding_function_selector_version": 1 if args.target == "bindings" else None,
        "binding_function_selector": "sum(input bytes) & 3: bit0=emit definitions, bit1=exclude inline functions; second generation"
        if args.target == "bindings"
        else None,
        "binding_derive_selector": "(sum(input bytes) >> 11) & 15: bit0=Copy, bit1=Debug, bit2=Default, bit3=Eq; second generation rustifies enums"
        if args.target == "bindings"
        else None,
        "query_dialect_selector": "sum(input bytes) & 1: 0=gnu, 1=clang"
        if args.target == "preprocess"
        else None,
        "trigraph_selector": "sum(input bytes) & 0x100 != 0"
        if args.target == "preprocess"
        else None,
        "preprocessor_selector_version": 4 if args.target == "preprocess" else None,
        "macro_definition_history_selector": "sum(input bytes) & 4 != 0"
        if args.target == "preprocess"
        else None,
        "scope_punctuator_selector": "sum(input bytes) & 2 != 0"
        if args.target == "preprocess"
        else None,
        "comment_policy_selector": "(sum(input bytes) >> 9) % 5: 0=enabled, 1=gcc-c90-compile, 2=gcc-c90-preprocess, 3=clang-c90-compile, 4=clang-c90-preprocess"
        if args.target == "preprocess"
        else None,
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "rustc": capture(["rustc", "-Vv"], root),
        "cargo_fuzz": capture(["cargo", "fuzz", "--version"], root),
        "sanitizer": "address",
        "sanitizer_options": {
            key: os.environ.get(key) for key in ("ASAN_OPTIONS", "LSAN_OPTIONS")
        },
        "status": "building",
    }

    def save():
        (output / "evidence.json").write_text(json.dumps(report, indent=2) + "\n")

    save()
    try:
        host = next(
            line[6:]
            for line in report["rustc"].splitlines()
            if line.startswith("host: ")
        )
        build = [
            "cargo",
            "fuzz",
            "build",
            args.target,
            "--target",
            host,
            "--sanitizer",
            "address",
        ]
        report["build_command"] = build
        with (output / "build.log").open("wb") as log:
            subprocess.run(
                build, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True
            )
        metadata = json.loads(
            capture(
                [
                    "cargo",
                    "metadata",
                    "--manifest-path",
                    "fuzz/Cargo.toml",
                    "--format-version",
                    "1",
                    "--no-deps",
                ],
                root,
            )
        )
        binary = output / args.target
        shutil.copy2(
            Path(metadata["target_directory"]) / host / "release" / args.target, binary
        )
        if "__asan_init" not in capture(["nm", str(binary)], root):
            raise ValueError(
                "built fuzzer does not contain AddressSanitizer initialization"
            )
        report["binary_sha256"] = sha256(binary)
        corpus = root / "fuzz" / "corpus" / args.target
        corpus.mkdir(parents=True, exist_ok=True)
        for path in sorted((root / "fuzz" / "seeds" / args.target).glob("*.h")):
            seeds = (
                seed_preprocessor_policies(path.read_bytes())
                if args.target == "preprocess"
                else seed_profiles(path.read_bytes(), report["profiles"], modes=8)
            )
            for data in seeds:
                (corpus / hashlib.sha256(data).hexdigest()).write_bytes(data)
        (corpus / "invalid-utf8").write_bytes(b"int valid_prefix;\xff")
        report["initial_corpus"] = archive_initial_corpus(corpus, output)
        report["initial_archive_sha256"] = sha256(output / "initial-corpus.tar.gz")
        artifacts = output / "artifacts"
        artifacts.mkdir()
        shutil.copy2(root / "fuzz" / "c.dict", output / "c.dict")
        command = [
            str(binary),
            str(corpus),
            f"-dict={output / 'c.dict'}",
            f"-max_total_time={args.seconds}",
            f"-max_len={8192 if args.target == 'bindings' else 16384}",
            "-len_control=0",
            "-timeout=5",
            "-rss_limit_mb=1024",
            "-print_final_stats=1",
            f"-seed={args.seed}",
            f"-artifact_prefix={artifacts}/",
        ]
        report.update(command=command, status="running")
        save()
        report.update(run_fuzzer(command, root, output, args.seconds))
        report["artifacts"] = corpus_manifest(artifacts)
        report["final_corpus"] = corpus_manifest(corpus)
        report["source_unchanged"] = source_manifest(root) == source
        report["status"] = "passed" if campaign_passed(report) else "failed"
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        report.update(status="failed", error=str(error))
    finally:
        report["finished_utc"] = datetime.datetime.now(
            datetime.timezone.utc
        ).isoformat()
        save()
    print(
        json.dumps(
            {
                key: report.get(key)
                for key in ("target", "status", "statistics", "error")
            }
        )
    )
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
