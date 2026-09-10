#!/usr/bin/env python3
"""Build the comparison harness against an explicit Toucan checkout."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_files(source):
    paths = subprocess.check_output(
        ['git', 'ls-files', '-c', '-o', '--exclude-standard', '--', 'crates',
         'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'],
        cwd=source, text=True,
    ).splitlines()
    return {name: sha(source / name) for name in sorted(set(paths))
            if (source / name).is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--build-dir', type=Path, required=True)
    parser.add_argument('--cargo-home', type=Path)
    args = parser.parse_args()
    source = args.source.resolve()
    output = args.output.resolve()
    target = args.target_dir.resolve()
    shared = args.build_dir.resolve()
    if target == shared or target in shared.parents or shared in target.parents:
        parser.error('target and shared build directories must be separate')
    output.mkdir(parents=True, exist_ok=False)
    harness = output / 'harness'
    shutil.copytree(ROOT / 'harness', harness)
    manifest = (harness / 'Cargo.toml.in').read_text()
    for name in ['toucan', 'toucan_bindgen', 'toucan_parser']:
        manifest = manifest.replace(
            json.dumps('__TOUCAN_SOURCE__/crates/' + name),
            json.dumps(str(source / 'crates' / name)),
        )
    (harness / 'Cargo.toml').write_text(manifest)
    before = source_files(source)
    result = {'status': 'failed', 'source': str(source), 'source_files': before,
              'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip(),
              'harness_main_sha256': sha(harness / 'src/main.rs'),
              'counter_sha256': sha(harness / 'src/counter.rs'), 'started_at': time.time()}
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_BUILD_DIR=str(shared), CARGO_INCREMENTAL='0')
    if args.cargo_home:
        env['CARGO_HOME'] = str(args.cargo_home.resolve())
    for key in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER']:
        env.pop(key, None)
    try:
        for mode, features in [('normal', []), ('allocations', ['--features', 'allocations'])]:
            command = ['cargo', '+ohm', '-Zohm-defaults=no', 'build', '--manifest-path',
                       str(harness / 'Cargo.toml'), '--release', '--locked', '--offline', *features]
            with (output / (mode + '.build.log')).open('w') as log:
                subprocess.run(command, cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            if source_files(source) != before:
                raise RuntimeError('source changed during build; repeat with a stable checkout')
            binary = output / mode
            shutil.copy2(target / 'release/toucan-parser-comparison', binary)
            result[mode] = {'binary': str(binary), 'sha256': sha(binary), 'command': command}
        result['status'] = 'passed'
    finally:
        (output / 'build.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'status': result['status'], 'output': str(output)}))


if __name__ == '__main__':
    main()
