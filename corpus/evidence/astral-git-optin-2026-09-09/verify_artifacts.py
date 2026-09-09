#!/usr/bin/env python3
"""Independently audit retained native application acceptance artifacts; no builds."""

from pathlib import Path
import argparse, hashlib, json, re, subprocess, tarfile, tomllib, zipfile
from pathlib import PurePosixPath

if not __debug__:
    raise RuntimeError("Run without Python optimization so audit checks remain active")

parser = argparse.ArgumentParser()
parser.add_argument("--project", choices=["uv", "ty"], required=True)
parser.add_argument("--artifact", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--repository", type=Path, required=True)
parser.add_argument("--project-archives", type=Path, required=True)
args = parser.parse_args()
project = args.project
output = args.output
output.mkdir(parents=True, exist_ok=True)
base = args.artifact / "__w/toucan/toucan/results/optin"
accept = base / "acceptance"
evidence = json.loads((accept / "evidence.json").read_text())
commit = (base / "commit.txt").read_text().strip()
expected_commit = "d00076d620c34b42c17a474e32e41090288e4483"
assert commit == expected_commit
assert (
    evidence["status"] == "passed" and evidence["target"] == "x86_64-unknown-linux-gnu"
)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def local(path):
    prefix = "/__w/toucan/toucan/results/optin/"
    assert path.startswith(prefix), path
    return base / path.removeprefix(prefix)


def git_file(path, revision=None):
    return subprocess.check_output(
        ["git", "show", (revision or commit) + ":" + path], cwd=args.repository
    )


assert evidence["frontend_mode"] == "git"
assert evidence["preparation"]["frontend_mode"] == "git"
assert evidence["preparation"]["local_override"] is None
pin = "85bf1ad6dcbc5840ade11bf8798785b6da13260a"
source_id = "git+https://github.com/astral-sh/toucan?rev=" + pin + "#" + pin
expected_frontend_names = {
    "toucan", "toucan_bindgen", "toucan_bindings", "toucan_layout", "toucan_parser",
    "toucan_preprocessor", "toucan_semantic", "toucan_source", "toucan_target",
}
prefetched = json.loads((base / "git-source.json").read_text())
assert prefetched == evidence["preparation"]["git_source"]
git_source = evidence["git_source"]
for key in ["root", "head", "source", "verified_source_files", "source_inventory_sha256", "packages"]:
    assert prefetched[key] == git_source[key], key
assert git_source["head"] == pin and git_source["source"] == source_id
frontend_root = PurePosixPath(git_source["root"])
assert frontend_root.is_absolute() and ".." not in frontend_root.parts
assert "/git/checkouts/" in str(frontend_root) and frontend_root.name == pin[:7]
assert evidence["preparation"]["toucan_source"] == str(frontend_root)
assert evidence["preparation"]["integration_git_revision"] == pin
assert len(git_source["packages"]) == 9
assert {p["name"] for p in git_source["packages"].values()} == expected_frontend_names
for package in git_source["packages"].values():
    assert package["manifest_path"] == str(frontend_root / "crates" / package["name"] / "Cargo.toml")
    for target in package["target_sources"]:
        path = PurePosixPath(target)
        assert path.is_relative_to(frontend_root / "crates" / package["name"])
        assert ".." not in path.parts
metadata = json.loads((accept / "toucan-metadata.stdout").read_text())
metadata_frontend_rows = [p for p in metadata["packages"] if p["name"] == "toucan" or p["name"].startswith("toucan_")]
assert len(metadata_frontend_rows) == 9
metadata_frontend = {p["id"]: p for p in metadata_frontend_rows}
assert set(metadata_frontend) == set(git_source["packages"])
for identity, package in metadata_frontend.items():
    reported = git_source["packages"][identity]
    assert package["source"] == source_id and package["version"] == "0.0.1"
    assert package["name"] == reported["name"]
    assert package["manifest_path"] == reported["manifest_path"]
    assert sorted(t["src_path"] for t in package["targets"]) == reported["target_sources"]

assert (
    evidence["project"] == json.loads(git_file("corpus/consumers/astral.json"))[project]
)
driver = git_file("scripts/verify_astral_optin.py")
assert sha(driver) == evidence["driver_sha256"]
(output / "verified-driver.py").write_bytes(driver)
for relative in ["corpus/consumers/zstd-optin/git_source.py", "corpus/consumers/zstd-optin/prepare.py", ".github/workflows/astral-optin.yml"]:
    (output / ("verified-" + Path(relative).name)).write_bytes(git_file(relative))
assert (
    "1.98.1" in evidence["rustc_version"]
    and "ohm" not in evidence["rustc_version"].lower()
)
assert all(c["exit_code"] == 0 for c in evidence["commands"])
for c in evidence["commands"]:
    assert file_sha(local(c["stdout"])) == c["stdout_sha256"]
    assert file_sha(local(c["stderr"])) == c["stderr_sha256"]
    assert not any("+ohm" in a or "-Zohm" in a for a in c["command"])
labels = {Path(c["stdout"]).stem: c for c in evidence["commands"]}
assert len(labels) == len(evidence["commands"])
positions = {Path(c["stdout"]).stem: i for i, c in enumerate(evidence["commands"])}
for name, operation in [("fetch-upstream-inputs", "fetch"), ("fetch-toucan-inputs", "fetch"), ("toucan-metadata", "metadata")]:
    assert name in labels
    entry = labels[name]
    assert entry["command"][:2] == ["cargo", operation]
    assert entry["cwd"] == evidence["preparation"]["projects"][project]["source"]
    assert "--locked" in entry["command"]
    target_flag = "--filter-platform" if operation == "metadata" else "--target"
    index = entry["command"].index(target_flag)
    assert entry["command"][index + 1] == evidence["target"]
assert positions["fetch-upstream-inputs"] < positions["upstream-build"]
assert positions["toucan-metadata"] < positions["fetch-toucan-inputs"] < positions["toucan-build"]
assert "--features" in labels["toucan-metadata"]["command"]
index = labels["toucan-metadata"]["command"].index("--features")
assert labels["toucan-metadata"]["command"][index + 1] == project + "/toucan-zstd"
for c in evidence["commands"]:
    if c["command"][0] == "cargo" and any(x in c["command"] for x in ["build", "test"]):
        assert "--offline" in c["command"]
        assert "--locked" in c["command"]
for phase in ["libclang_before", "libclang_after"]:
    scan = evidence[phase]
    assert scan["found"] == [] and scan["library_lookup"] is None
    assert set(scan["searched_roots"]) == {"/opt", "/usr/lib", "/usr/local/lib"}
packages = (base / "bootstrap/packages.txt").read_text().splitlines()
assert not any("libclang" in line.split()[0] for line in packages if line.strip())
assert any(line.startswith("libzstd1:") for line in packages)

inventory_raw = git_file("corpus/consumers/zstd-optin/source-digests.json")
assert sha(inventory_raw) == evidence["preparation"]["source_inventory_sha256"]
inventory = json.loads(inventory_raw)
assert len(inventory) == evidence["preparation"]["verified_source_files"] == 630
paths = list(inventory)
batch = subprocess.check_output(
    ["git", "cat-file", "--batch"],
    input=("\n".join(commit + ":" + p for p in paths) + "\n").encode(),
    cwd=args.repository,
)
pos = 0
for path in paths:
    end = batch.index(b"\n", pos)
    header = batch[pos:end].split()
    assert header[1] == b"blob"
    size = int(header[2])
    pos = end + 1
    data = batch[pos : pos + size]
    assert sha(data) == inventory[path], path
    pos += size + 1
assert pos == len(batch)
# Independently inspect all 630 pinned Git source blobs, as well as the tested driver checkout above.
for path, checksum in inventory.items():
    assert sha(git_file(path, pin)) == checksum, (pin, path)
assert git_source["verified_source_files"] == 630
assert git_source["source_inventory_sha256"] == sha(inventory_raw)
for package in git_source["packages"].values():
    for target in package["target_sources"]:
        assert str(PurePosixPath(target).relative_to(frontend_root)) in inventory
for name, checksum in evidence["preparation"]["patch_sha256"].items():
    assert sha(git_file("corpus/consumers/zstd-optin/patches/" + name)) == checksum

archive = args.project_archives / ("ruff.tar.gz" if project == "ty" else "uv.tar.gz")
assert file_sha(archive) == evidence["project"]["archive_sha256"]
original = {}
normalizations = evidence["upstream_archive"]["archive_link_normalizations"]
with tarfile.open(archive) as tar:
    for member in tar:
        rel = "/".join(Path(member.name).parts[1:])
        if member.isfile():
            data = tar.extractfile(member).read()
            original[rel] = sha(data)
            if rel == "Cargo.lock":
                archive_lock = data
        elif member.issym():
            target = member.linkname
            if rel in normalizations:
                assert normalizations[rel]["archive"] == target
                target = normalizations[rel]["extracted"]
                assert target == member.linkname.rstrip("/")
            original[rel] = "symlink:" + target
assert original == evidence["upstream_source_inventory"]
assert len(original) == evidence["upstream_archive"]["source_files"]
assert archive_lock == (accept / "upstream.lock").read_bytes()
for rel, checksum in evidence["preparation"]["projects"][project][
    "original_manifest_sha256"
].items():
    assert original[rel] == checksum
assert set(evidence["changed_project_manifests"]) == (
    {"Cargo.toml", "crates/uv-extract/Cargo.toml", "crates/uv/Cargo.toml"}
    if project == "uv"
    else {
        "Cargo.toml",
        "crates/ty_vendored/Cargo.toml",
        "crates/ty_project/Cargo.toml",
        "crates/ty/Cargo.toml",
    }
)

old = tomllib.loads((accept / "upstream.lock").read_text())["package"]
new = tomllib.loads((accept / "toucan.lock").read_text())["package"]
old = {(p["name"], p["version"]): p for p in old}
new = {(p["name"], p["version"]): p for p in new}
assert (
    old.keys() <= new.keys()
    and len(old) == evidence["lock"]["existing_packages_preserved"]
)


def edges(p, mapping):
    result = set()
    for text in p.get("dependencies", []):
        parts = text.split()
        matched = [
            key
            for key in mapping
            if key[0] == parts[0] and (len(parts) == 1 or key[1] == parts[1])
        ]
        assert len(matched) == 1, (text, matched)
        result.add(matched[0])
    return result


for key, p in old.items():
    q = new[key]
    if key[0] != "zstd-sys":
        assert (p.get("source"), p.get("checksum")) == (
            q.get("source"),
            q.get("checksum"),
        )
    else:
        assert "source" not in q and "checksum" not in q
    assert edges(p, old) <= edges(q, new), key
for phase in ["upstream", "toucan"]:
    key = "upstream_sha256" if phase == "upstream" else "builder_sha256"
    assert file_sha(accept / (phase + ".lock")) == evidence["lock"][key]
    tree = evidence[phase + "_tree"]
    assert file_sha(local(tree["path"])) == tree["sha256"]
    names = {
        line.split(" v")[0] for line in local(tree["path"]).read_text().splitlines()
    }
    assert not {"bindgen", "clang-sys"} & names
    assert ("toucan_bindgen" in names) == (phase == "toucan")

library = "uv-extract" if project == "uv" else "ty_vendored"
count = 19 if project == "uv" else 2
bindings = []
frontend = []
binary_hashes = {}
for phase in ["upstream", "toucan"]:
    tests = evidence["library_tests"][phase]
    assert len(tests) == 1
    test = tests[0]
    assert test["package"] == library and test["execution"]["exit_code"] == 0
    stdout = test["execution"]["stdout"]
    named = dict(re.findall(r"^test (\S+) \.\.\. (ok|ignored)$", stdout, re.M))
    assert len(named) == count and set(named.values()) == {"ok"}
    assert test["result"] == {
        "passed": count,
        "ignored": 0,
        "tests": dict(sorted(named.items())),
    }
    assert (
        f"test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;"
        in stdout
    )
    for suffix in ["build", "tests-" + library]:
        rows = [
            json.loads(line)
            for line in (accept / (phase + "-" + suffix + ".stdout"))
            .read_text()
            .splitlines()
        ]
        assert rows[-1] == {"reason": "build-finished", "success": True}
        artifacts = [r for r in rows if r.get("reason") == "compiler-artifact"]
        if suffix == "build":
            selected = [
                r
                for r in artifacts
                if r["target"]["name"] == project
                and "bin" in r["target"]["kind"]
                and r.get("executable")
            ]
            assert len(selected) == 1
            for key in ["package_id", "manifest_path", "profile", "executable"]:
                assert selected[0][key] == evidence[phase + "_binary"][key]
            assert ("toucan-zstd" in selected[0]["features"]) == (phase == "toucan")
        else:
            selected = [
                r
                for r in artifacts
                if r.get("executable") == test["execution"]["command"][0]
                and r["profile"]["test"]
            ]
            assert len(selected) == 1
            assert selected[0]["target"]["name"] == library.replace("-", "_")

        names = {r["target"]["name"] for r in artifacts}
        assert not {"bindgen", "clang_sys"} & names
        front = [
            r
            for r in artifacts
            if (
                r["target"]["name"] == "toucan"
                or r["target"]["name"].startswith("toucan_")
            )
            and "lib" in r["target"]["kind"]
        ]
        assert len(front) == (9 if phase == "toucan" else 0)
        for f in front:
            name = f["target"]["name"]
            assert f["package_id"] in metadata_frontend
            package = metadata_frontend[f["package_id"]]
            assert name == package["name"]
            assert f["manifest_path"] == package["manifest_path"]
            assert f["target"]["src_path"] in [t["src_path"] for t in package["targets"]]
            if suffix == "build":
                assert f["fresh"] is False
        if phase == "toucan":
            recorded_frontend = evidence["frontend_artifacts"] if suffix == "build" else test["frontend_artifacts"]
            assert len(recorded_frontend) == len(front) == 9
            assert {row["package_id"] for row in recorded_frontend} == set(metadata_frontend)
            assert {row["package_id"] for row in front} == set(metadata_frontend)
            for row in recorded_frontend:
                matching = [f for f in front if f["package_id"] == row["package_id"]]
                assert len(matching) == 1
                actual = matching[0]
                assert row["manifest_path"] == actual["manifest_path"]
                assert row["src_path"] == actual["target"]["src_path"]
                assert row["features"] == actual["features"] and row["fresh"] == actual["fresh"]
                assert set(row["files"]) == set(actual["filenames"])
                assert row["files"] and all(re.fullmatch(r"[0-9a-f]{64}", h) for h in row["files"].values())
            frontend.append(
                {
                    "build": suffix,
                    "recorded_library_hashes": recorded_frontend,
                    "library_bytes_uploaded": False,
                    "packages": [
                        {k: f[k] for k in ["package_id", "manifest_path", "fresh"]}
                        for f in front
                    ],
                }
            )
        records = (
            evidence[phase + "_bindings"] if suffix == "build" else test["bindings"]
        )
        sys_artifacts = [r for r in artifacts if r["target"]["name"] == "zstd_sys"]
        assert len(sys_artifacts) == len(records)
        for record in records:
            dep = local(record["dep_info"])
            assert file_sha(dep) == record["dep_info_sha256"]
            text = dep.read_text()
            match = [
                r
                for r in sys_artifacts
                if r["package_id"] == record["package_id"]
                and Path(r["filenames"][0]).stem.removeprefix("lib") == dep.stem
            ]
            assert len(match) == 1
            assert match[0]["features"] == record["features"]
            if phase == "toucan":
                assert record["features"] == ["std", "toucan"]
                file = local(record["generated_binding"])
                assert file_sha(file) == record["generated_binding_sha256"]
                assert file.read_text().startswith(
                    "// Generated by Toucan for x86_64-unknown-linux-gnu.\n"
                )
                assert record["original_out_dir_binding"] in text
                assert not re.search(r"/src/bindings_(?:zstd|zdict)[^\s]*\.rs", text)
            else:
                assert record["features"] == ["std"] and "/src/bindings_zstd.rs" in text
            bindings.append({"phase": phase, "build": suffix, **record})
    binary = accept / (project + "-" + phase)
    binary_hashes[phase] = file_sha(binary)
    assert binary_hashes[phase] == evidence[phase + "_binary"]["sha256"]
    header = binary.open("rb").read(20)
    assert header[:5] == b"\x7fELF\x02" and header[18:20] == b"\x3e\x00"
assert (
    evidence["library_tests"]["upstream"][0]["result"]
    == evidence["library_tests"]["toucan"][0]["result"]
)
if project == "ty":
    prefix = "/tmp/toucan-optin-cache/ty-target/"
    for records in [
        evidence["toucan_bindings"],
        evidence["library_tests"]["toucan"][0]["bindings"],
    ]:
        # Host OUT_DIR starts directly with debug; the target instance has its triple first.
        assert any(
            r["original_out_dir_binding"].startswith(prefix + "debug/") for r in records
        )
        assert any(
            r["original_out_dir_binding"].startswith(
                prefix + "x86_64-unknown-linux-gnu/debug/"
            )
            for r in records
        )


runtime = evidence["runtime"]
if project == "uv":
    assert file_sha(accept / "runtime/fixture.whl") == runtime["wheel_sha256"]
    assert (
        file_sha(accept / "runtime/fixture.whl.zst") == runtime["encoded_wheel_sha256"]
    )
    assert (
        file_sha(accept / "runtime/truncated.whl.zst")
        == runtime["truncated_frame"]["sha256"]
    )
    with zipfile.ZipFile(accept / "runtime/fixture.whl") as wheel:
        expected = {
            name: sha(wheel.read(name))
            for name in runtime["upstream"]["installed_fixture_sha256"]
        }
    for phase, key, folder in [
        ("upstream", "upstream", "uv-upstream"),
        ("toucan", "generated", "uv-generated"),
    ]:
        case = runtime[key]
        assert case["exit_code"] == 0 and case["binary_sha256"] == binary_hashes[phase]
        assert case["installed_fixture_sha256"] == expected
        for path, checksum in expected.items():
            assert (
                file_sha(accept / "runtime" / folder / "installed" / path) == checksum
            )
        assert (
            case["import"]["exit_code"] == 0
            and case["import"]["stdout"] == "toucan-zstd-workspace-probe\n"
            and not case["import"]["stderr"]
        )
        assert any(r["method"] == "GET" for r in case["requests"])
        bad = runtime["truncated_frame"][key]
        assert bad["exit_code"] == 1 and bad["fixture_installed"] is False
        assert (
            bad["binary_sha256"] == binary_hashes[phase]
            and "unexpected end of file" in bad["stderr"]
        )
    assert (
        runtime["truncated_frame"]["upstream"]["stderr"]
        == runtime["truncated_frame"]["generated"]["stderr"]
    )
    runtime_summary = {
        "installed_files": expected,
        "wheel_sha256": runtime["wheel_sha256"],
        "zstd_wheel_sha256": runtime["encoded_wheel_sha256"],
        "imports_match": True,
        "truncated_frame_rejected_without_installation": True,
    }
else:
    assert runtime["upstream"]["binary_sha256"] == binary_hashes["upstream"]
    assert runtime["generated"]["binary_sha256"] == binary_hashes["toucan"]
    for key in ["valid", "invalid"]:
        a = runtime["upstream"]["cases"][key]
        b = runtime["generated"]["cases"][key]
        assert all(
            a[field] == b[field]
            for field in ["exit_code", "stdout", "stderr", "source_sha256"]
        )
        assert a["exit_code"] == (0 if key == "valid" else 1)
        assert file_sha(local(a["command"][-1])) == a["source_sha256"]
    runtime_summary = {"valid_invalid_diagnostics_match": True}

report = {
    "status": "passed",
    "project": project,
    "run_id": 34394853038,
    "tested_commit": commit,
    "project_revision": evidence["project"]["revision"],
    "target": evidence["target"],
    "rustc_version": evidence["rustc_version"],
    "driver_sha256": evidence["driver_sha256"],
    "command_count": len(evidence["commands"]),
    "libclang_before": evidence["libclang_before"],
    "libclang_after": evidence["libclang_after"],
    "installed_libclang_packages": [],
    "verified_frontend_source_files": len(inventory),
    "frontend_mode": "git",
    "frontend_git_revision": pin,
    "frontend_git_source": git_source,
    "frontend_rlib_hash_verification": "Recorded CI hashes retained and syntax-checked; library identities and filenames match Cargo rows. Library bytes were not uploaded and cannot be independently rehashed.",
    "frontend_artifacts": frontend,
    "original_project_files": len(original),
    "original_locked_packages": len(old),
    "original_package_versions_and_dependency_edges_preserved": True,
    "library_tests_per_build": count,
    "library_test_results": evidence["library_tests"]["toucan"][0]["result"],
    "binary_sha256": binary_hashes,
    "binding_inputs": bindings,
    "runtime": runtime_summary,
    "limitations": [
        "Linux x86-64 zstd feature only; AWS-LC generation is separate.",
        "No performance comparison; dev/test builds have debug information disabled.",
        "Post-build source stability, actual Git HEAD and binding freshness were checked by the hash-verified CI driver/helper; live CI filesystem was not re-read during artifact audit.",
        "All nine frontend library identities, manifests, source paths and artifact filenames match retained Cargo rows. Recorded hashes are retained and syntax-checked; their .rlib files were not uploaded, so library hashes were not independently recomputed.",
        "The pin's 630 Git blobs were independently checked locally. The fetched CI checkout is identified by the retained source report and hash-verified helper, not an uploaded checkout.",
    ],
}
(output / "audit.json").write_text(json.dumps(report, indent=2) + "\n")
(output / "evidence.json").write_bytes((accept / "evidence.json").read_bytes())
(output / "verified-source-digests.json").write_bytes(inventory_raw)
(output / "git-source.json").write_bytes((base / "git-source.json").read_bytes())
print(
    json.dumps(
        {
            k: v
            for k, v in report.items()
            if k
            in [
                "status",
                "project",
                "tested_commit",
                "command_count",
                "original_project_files",
                "original_locked_packages",
                "library_tests_per_build",
                "binary_sha256",
            ]
        },
        indent=2,
    )
)
