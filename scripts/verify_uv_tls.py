#!/usr/bin/env python3
"""Exercise pinned uv HTTPS downloads and verify its compiled AWS-LC dependency path."""

from __future__ import annotations

import argparse
import contextlib
import hashlib
import http.server
import json
import os
import platform
import shutil
import ssl
import sys
import threading
from itertools import pairwise
from pathlib import Path

import tomllib

sys.path.insert(0, str(Path(__file__).resolve().parent))
from verify_astral_consumers import (
    check_archive_source,
    digest,
    run,
    source_inventory,
    wheel,
)
from verify_aws_lc_consumer import source_hashes, verify_registry_source

ROOT = Path(__file__).resolve().parents[1]
WHEEL_NAME = "toucan_consumer_probe-1.0.0-py3-none-any.whl"


def certificates(directory: Path) -> dict:
    """Create a process-local test trust root, server certificate and unrelated root."""
    directory.mkdir()
    commands = []

    def openssl(arguments: list[str]) -> None:
        commands.append(run(["openssl", *arguments], directory))

    openssl(["version", "-a"])
    for name in ("trusted", "unrelated"):
        openssl(
            [
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-sha256",
                "-days",
                "2",
                "-subj",
                f"/CN=Toucan {name} test CA",
                "-keyout",
                f"{name}.key",
                "-out",
                f"{name}.pem",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
                "-addext",
                "keyUsage=critical,keyCertSign,cRLSign",
            ]
        )
    openssl(
        [
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-sha256",
            "-subj",
            "/CN=localhost",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
        ]
    )
    (directory / "server.ext").write_text(
        "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\n"
        "extendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost\n"
    )
    openssl(
        [
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "trusted.pem",
            "-CAkey",
            "trusted.key",
            "-set_serial",
            "1",
            "-days",
            "2",
            "-sha256",
            "-extfile",
            "server.ext",
            "-out",
            "server.pem",
        ]
    )
    openssl(
        [
            "verify",
            "-CAfile",
            "trusted.pem",
            "-verify_hostname",
            "localhost",
            "server.pem",
        ]
    )
    for key in directory.glob("*.key"):
        key.chmod(0o600)
    return {
        "commands": commands,
        "certificates_sha256": {
            path.name: digest(path) for path in sorted(directory.glob("*.pem"))
        },
    }


@contextlib.contextmanager
def https_wheel(data: bytes, certs: Path, version: ssl.TLSVersion):
    requests: list[dict] = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def serve(self, body: bool) -> None:
            requests.append(
                {
                    "method": self.command,
                    "path": self.path,
                    "tls_version": self.connection.version(),
                    "cipher": self.connection.cipher(),
                    "headers": {
                        name: self.headers[name]
                        for name in ("User-Agent", "Accept-Encoding", "Range")
                        if name in self.headers
                    },
                }
            )
            if self.path != "/" + WHEEL_NAME:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            if body:
                self.wfile.write(data)

        def do_GET(self):
            self.serve(True)

        def do_HEAD(self):
            self.serve(False)

        def log_message(self, *_args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = context.maximum_version = version
    context.load_cert_chain(certs / "server.pem", certs / "server.key")
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield server.server_port, requests
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def runtime(binary: Path, output: Path, python: str) -> dict:
    output.mkdir()
    certs = output / "certificates"
    evidence = {
        "binary_sha256": digest(binary),
        "trust": certificates(certs),
        "cases": {},
    }
    data, expected_files = wheel()
    (output / WHEEL_NAME).write_bytes(data)
    evidence["wheel_sha256"] = hashlib.sha256(data).hexdigest()
    # Certificate trust is scoped to these child processes. Proxy, client-cert,
    # index, insecure-host and shared-cache settings cannot alter the fixture.
    environment = {
        name: value
        for name, value in os.environ.items()
        if not name.startswith(("UV_", "PIP_", "SSL_CERT_"))
        and name not in {"SSL_CLIENT_CERT", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE"}
        and not name.lower().endswith("_proxy")
    }
    environment.update(
        UV_NO_CONFIG="1",
        UV_NO_CACHE="1",
        UV_PYTHON_DOWNLOADS="never",
        UV_HTTP_RETRIES="0",
        PYTHONDONTWRITEBYTECODE="1",
        NO_PROXY="localhost,127.0.0.1",
        no_proxy="localhost,127.0.0.1",
    )
    for version in (ssl.TLSVersion.TLSv1_2, ssl.TLSVersion.TLSv1_3):
        for case in ("trusted", "untrusted", "wrong-host"):
            directory = output / f"{version.name}-{case}"
            directory.mkdir()
            installed = directory / "installed"
            trust = certs / ("unrelated.pem" if case == "untrusted" else "trusted.pem")
            case_env = environment | {"SSL_CERT_FILE": str(trust)}
            with https_wheel(data, certs, version) as (port, requests):
                host = "127.0.0.1" if case == "wrong-host" else "localhost"
                url = f"https://{host}:{port}/{WHEEL_NAME}"
                result = run(
                    [
                        str(binary),
                        "--no-progress",
                        "--color",
                        "never",
                        "pip",
                        "install",
                        "--no-deps",
                        "--no-index",
                        "--link-mode",
                        "copy",
                        "--python",
                        python,
                        "--target",
                        str(installed),
                        url,
                    ],
                    directory,
                    env=case_env,
                    expected=0 if case == "trusted" else 2,
                )
            result["certificate_environment"] = {"SSL_CERT_FILE": str(trust)}
            result["requests"] = requests
            evidence["cases"][directory.name] = result
            (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
            if case == "trusted":
                assert requests and any(
                    request["method"] == "GET" for request in requests
                )
                assert {request["tls_version"] for request in requests} == {
                    version.name.replace("_", ".")
                }
                checked = {}
                for name, contents in expected_files.items():
                    if name.endswith("/RECORD"):
                        continue
                    actual = installed / name
                    assert actual.read_bytes() == contents, (
                        f"installed bytes differ: {name}"
                    )
                    checked[name] = digest(actual)
                result["installed_fixture_sha256"] = checked
                result["import"] = run(
                    [
                        python,
                        "-I",
                        "-c",
                        "import sys; sys.path.insert(0, sys.argv[1]); import toucan_consumer_probe as p; print(p.VALUE)",
                        str(installed),
                    ],
                    directory,
                    env=case_env,
                )
                assert result["import"]["stdout"] == "toucan-zstd-workspace-probe\n"
            else:
                assert not requests, (
                    "uv fetched wheel bytes after certificate rejection"
                )
                assert not (installed / "toucan_consumer_probe").exists()
                expected = (
                    "UnknownIssuer"
                    if case == "untrusted"
                    else 'certificate not valid for name "127.0.0.1"'
                )
                assert expected in result["stderr"], (
                    f"failure was not the expected certificate rejection: {result['stderr']}"
                )
                result["certificate_rejection"] = expected
            (output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    return evidence


def audit(args: argparse.Namespace, output: Path) -> dict:
    """Tie the executable to the pinned archive, Cargo features and consumed C bindings."""
    project = json.loads((ROOT / "corpus/consumers/astral.json").read_text())["uv"]
    assert digest(args.archive) == project["archive_sha256"]
    source = check_archive_source(args.archive, args.source)
    archive_lock = source.pop("archive_lock")
    source_before = source_inventory(args.source)
    assert digest(args.binary) == args.binary_sha256
    messages = [json.loads(line) for line in args.cargo_log.read_text().splitlines()]
    artifacts = [
        message for message in messages if message.get("reason") == "compiler-artifact"
    ]
    uv = [
        row
        for row in artifacts
        if row["target"]["name"] == "uv"
        and row["target"]["kind"] == ["bin"]
        and row["executable"]
    ]
    assert len(uv) == 1
    assert (
        Path(uv[0]["manifest_path"]).resolve()
        == args.source.resolve() / "crates/uv/Cargo.toml"
    )
    selected = {}
    for name, version in (
        ("uv_client", None),
        ("reqwest", "0.13.4"),
        ("rustls", "0.23.43"),
        ("aws_lc_rs", "1.18.0"),
        ("aws_lc_sys", "0.44.0"),
    ):
        rows = [
            row
            for row in artifacts
            if row["target"]["name"] == name and row["target"]["kind"] == ["lib"]
        ]
        assert len(rows) == 1
        row = rows[0]
        if version is not None:
            assert row["package_id"].endswith("@" + version) or row[
                "package_id"
            ].endswith("#" + version)
        rlib = next(Path(path) for path in row["filenames"] if path.endswith(".rlib"))
        dep = rlib.with_name(rlib.stem.removeprefix("lib") + ".d")
        assert rlib.is_file() and dep.is_file()
        shutil.copyfile(dep, output / f"{name}.d")
        selected[name] = {
            "artifact": row,
            "rlib_sha256": digest(rlib),
            "dep_info_sha256": digest(dep),
        }
    client_source = args.source.resolve() / "crates/uv-client"
    assert (
        Path(selected["uv_client"]["artifact"]["manifest_path"]).parent == client_source
    )
    client_words = {
        str(Path(word) if Path(word).is_absolute() else args.source / word)
        for word in (output / "uv_client.d").read_text().split()
        if not word.endswith(":")
    }
    assert all(
        str(client_source / path) in client_words
        for path in ("src/base_client.rs", "src/tls.rs")
    )
    rustls_features = set(selected["rustls"]["artifact"]["features"])
    assert "aws_lc_rs" in rustls_features and "ring" not in rustls_features
    assert "tls12" in rustls_features
    assert not any(
        "native-tls" in feature
        for feature in selected["reqwest"]["artifact"]["features"]
    )
    assert selected["aws_lc_rs"]["artifact"]["features"] == [
        "aws-lc-sys",
        "prebuilt-nasm",
    ]
    sys = selected["aws_lc_sys"]["artifact"]
    expected_features = (
        ["prebuilt-nasm"]
        if args.bindings == "pregenerated"
        else ["bindgen", "prebuilt-nasm"]
    )
    assert sys["features"] == expected_features
    dep_words = (output / "aws_lc_sys.d").read_text().split()
    sys_source = Path(sys["manifest_path"]).parent
    rs_source = Path(selected["aws_lc_rs"]["artifact"]["manifest_path"]).parent
    original_sys = rs_source.parent / "aws-lc-sys-0.44.0"
    source_proof = {
        "aws-lc-rs": verify_registry_source(rs_source),
        "aws-lc-sys": verify_registry_source(original_sys),
    }
    expected_sys = source_proof["aws-lc-sys"]["files"]
    if args.bindings != "pregenerated":
        manifest = (original_sys / "Cargo.toml").read_text()
        assert manifest.count("prebuilt-nasm = []") == 1
        manifest = manifest.replace("prebuilt-nasm = []", 'prebuilt-nasm = ["bindgen"]')
        if args.bindings == "toucan":
            tested = tomllib.loads((sys_source / "Cargo.toml").read_text())[
                "build-dependencies"
            ]["bindgen"]
            assert tested["package"] == "toucan_bindgen"
            dependency = '[build-dependencies.bindgen]\nversion = "0.72.0"'
            assert manifest.count(dependency) == 1
            manifest = manifest.replace(
                dependency,
                '[build-dependencies.bindgen]\npackage = "toucan_bindgen"\npath = '
                + json.dumps(tested["path"]),
            )
            builder_rows = [
                row for row in artifacts if row["target"]["name"] == "toucan_bindgen"
            ]
            assert len(builder_rows) == 1
            assert (
                Path(builder_rows[0]["manifest_path"]).parent.resolve()
                == Path(tested["path"]).resolve()
            )
        expected_sys = expected_sys | {
            "Cargo.toml": hashlib.sha256(manifest.encode()).hexdigest()
        }
    assert source_hashes(sys_source) == expected_sys, (
        "AWS-LC source changes extend beyond the reviewed Cargo manifest edits"
    )
    assert str(sys_source / "src/lib.rs") in dep_words
    if args.bindings == "pregenerated":
        bindings = {Path(word) for word in dep_words if word.endswith("_crypto.rs")}
    else:
        assert not any(word.endswith("_crypto.rs") for word in dep_words)
        bindings = {
            Path(word)
            for word in dep_words
            if "/out/bindings.rs" in word and not word.endswith(":")
        }
        builds = [
            row
            for row in messages
            if row.get("reason") == "build-script-executed"
            and row["package_id"] == sys["package_id"]
        ]
        assert len(builds) == 1 and "use_bindgen_pregenerated" in builds[0]["cfgs"]
    assert len(bindings) == 1
    binding = bindings.pop()
    if args.bindings != "pregenerated":
        marker = (
            "Generated by Toucan"
            if args.bindings == "toucan"
            else "automatically generated by rust-bindgen 0.72.1"
        )
        assert marker in binding.read_text()
    shutil.copyfile(binding, output / "bindings.rs")
    command = [
        "cargo",
        "metadata",
        "--locked",
        "--offline",
        "--format-version=1",
        "--filter-platform",
        "x86_64-unknown-linux-gnu",
    ]
    # Configured source replacement must be the same one that produced the
    # artifact. Passing it here does not alter an upstream Cargo manifest.
    if not sys["package_id"].startswith("registry+"):
        command += [
            "--config",
            "patch.crates-io.aws-lc-sys.path=" + json.dumps(str(sys_source)),
        ]
    audit_source = output / "cargo-audit" / args.source.name
    shutil.copytree(args.source, audit_source, symlinks=True)
    if args.bindings == "pregenerated":
        (audit_source / "Cargo.lock").write_bytes(archive_lock)
    audit_before = source_inventory(audit_source)
    metadata_result = run(command, audit_source, timeout=600)
    (output / "cargo-metadata.json").write_text(metadata_result["stdout"])
    metadata = json.loads(metadata_result["stdout"])
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    ids = [
        selected[name]["artifact"]["package_id"].replace(
            str(args.source), str(audit_source)
        )
        for name in ("uv_client", "reqwest", "rustls", "aws_lc_rs", "aws_lc_sys")
    ]
    for before, after in pairwise(ids):
        assert after in {dep["pkg"] for dep in nodes[before]["deps"]}, (
            "Cargo dependency path differs"
        )
    compiled = {row["package_id"] for row in artifacts}
    root_id = next(
        package["id"]
        for package in metadata["packages"]
        if Path(package["manifest_path"]) == audit_source / "crates/uv/Cargo.toml"
    )
    pending = [[root_id]]
    visited = set()
    while pending:
        path = pending.pop(0)
        if path[-1] == ids[0]:
            break
        if path[-1] in visited:
            continue
        visited.add(path[-1])
        for dep in nodes[path[-1]]["deps"]:
            original_id = dep["pkg"].replace(str(audit_source), str(args.source))
            if original_id in compiled and any(
                kind["kind"] is None for kind in dep["dep_kinds"]
            ):
                pending.append([*path, dep["pkg"]])
    else:
        raise AssertionError(
            "the compiled uv normal-dependency graph does not reach uv-client"
        )
    assert source_inventory(args.source) == source_before, (
        "Cargo audit changed the pinned project source"
    )
    assert source_inventory(audit_source) == audit_before, (
        "Cargo audit changed its copied source or lock"
    )
    return {
        "source": source,
        "archive_sha256": digest(args.archive),
        "binary_sha256": args.binary_sha256,
        "cargo_log_sha256": digest(args.cargo_log),
        "compiled_path": selected,
        "uv_artifact": uv[0],
        "dependency_edges": list(pairwise(ids)),
        "uv_to_client_path": path,
        "upstream_wrapper_sources": source_proof,
        "bindings_kind": args.bindings,
        "bindings_sha256": digest(binding),
        "metadata_command": metadata_result["command"],
        "audited_lock_sha256": digest(audit_source / "Cargo.lock"),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--binary-sha256", required=True)
    parser.add_argument("--cargo-log", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument(
        "--bindings", choices=["pregenerated", "bindgen", "toucan"], required=True
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--python", default=sys.executable)
    args = parser.parse_args()
    if sys.platform != "linux" or platform.machine() != "x86_64":
        parser.error("this first fixture gate requires native x86-64 Linux")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True)
    source_files = [
        Path(__file__).resolve().parent / name
        for name in (
            "verify_uv_tls.py",
            "verify_aws_lc_consumer.py",
            "verify_astral_consumers.py",
        )
    ]
    evidence = {
        "status": "failed",
        "harness_sha256": digest(Path(__file__)),
        "harness_sources": {path.name: digest(path) for path in source_files},
        "metadata_toolchain": os.environ.get("RUSTUP_TOOLCHAIN"),
    }
    try:
        evidence["audit"] = audit(args, args.output)
        evidence["runtime"] = runtime(
            args.binary.resolve(), args.output / "runtime", args.python
        )
        assert {path.name: digest(path) for path in source_files} == evidence[
            "harness_sources"
        ]
        assert digest(args.binary) == args.binary_sha256
        evidence["status"] = "passed"
    except Exception as error:
        evidence["error"] = str(error)
        raise
    finally:
        (args.output / "evidence.json").write_text(
            json.dumps(evidence, indent=2) + "\n"
        )


if __name__ == "__main__":
    main()
