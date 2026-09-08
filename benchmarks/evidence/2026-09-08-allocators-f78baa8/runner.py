import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import tempfile
import time

ROOT = Path('/home/dev-user/code/oss/toucan')
CACHE = Path('/home/dev-user/.cache/toucan')
OUT = CACHE / 'allocator-comparison-f78baa8'
OUT.mkdir(exist_ok=False)
spec = importlib.util.spec_from_file_location('benchmark', ROOT / 'scripts/benchmark.py')
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)
commit = 'f78baa8dc399daceabeeead4878482fc7f53a33e'
source = CACHE / 'source-f78baa8'
binaries = {name: CACHE / 'conformance-bin' / filename for name, filename in [
    ('system', 'toucan-f78baa8'), ('jemalloc', 'toucan-f78baa8-jemalloc')]}
hashes = {name: benchmark.digest(path) for name, path in binaries.items()}
files = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', commit], cwd=ROOT, text=True).splitlines()
source_hashes = {}
for name in files:
    if name.startswith('crates/') or name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml']:
        original = subprocess.check_output(['git', 'show', f'{commit}:{name}'], cwd=ROOT)
        assert (source/name).read_bytes() == original, name
        source_hashes[name] = hashlib.sha256(original).hexdigest()
commands = {}
reports = {}
dependencies = set()
observations = {}
with tempfile.TemporaryDirectory(prefix='toucan-allocator-comparison-') as tmp:
    tmp = Path(tmp)
    for project in ['libgit2', 'sqlite', 'zlib', 'zstd']:
        config = json.loads((ROOT/f'benchmarks/evidence/2026-09-08-4ac7511/{project}.json').read_text())
        for name, binary in binaries.items():
            key = f'{project}/{name}'
            command = [str(binary), *config['commands']['toucan'][1:]]
            commands[key] = command
            report = tmp / 'report.json'
            benchmark.execute([*command, '--report', str(report)], 60)
            reports[key] = json.loads(report.read_text())
            dependencies.update(Path(path).resolve() for path in reports[key]['dependencies'])
            dependencies.add(Path(command[2]).resolve())
            observations[key] = []
    before = {str(path): benchmark.digest(path) for path in sorted(dependencies)}
    randomizer = random.Random(20260908)
    order = list(commands)
    for iteration in range(16):
        randomizer.shuffle(order)
        for position, key in enumerate(order):
            command = commands[key]
            rss = tmp / 'rss.txt'
            invocation = ['/usr/bin/time', '-f', '%M', '-o', str(rss), *command]
            start = time.perf_counter_ns()
            result = benchmark.execute(invocation, 60)
            observations[key].append({'iteration': iteration, 'order': position,
                'wall_ms': (time.perf_counter_ns()-start)/1e6,
                'peak_rss_kib': int(rss.read_text()), 'output_bytes': len(result.stdout),
                'output_sha256': hashlib.sha256(result.stdout).hexdigest(),
                'stderr': result.stderr.decode(errors='replace')})
    after = {str(path): benchmark.digest(path) for path in sorted(dependencies)}
    assert before == after
    assert hashes == {name: benchmark.digest(path) for name, path in binaries.items()}
    equality = {}
    for project in ['libgit2', 'sqlite', 'zlib', 'zstd']:
        equality[project] = len({sample['output_sha256'] for name in binaries for sample in observations[f'{project}/{name}']}) == 1
    assert all(equality.values())
    summary = {key: {field: statistics.median(sample[field] for sample in samples[1:])
        for field in ['wall_ms', 'peak_rss_kib']} for key, samples in observations.items()}
    data = {'schema_version': 1, 'source_commit': commit, 'source_files_sha256': source_hashes,
        'binary_sha256': hashes, 'build_features': {'system': [], 'jemalloc': ['performance-allocator']},
        'rustc': benchmark.version(['rustc','--version'],60), 'platform': platform.platform(),
        'cpu_model': benchmark.cpu_model(60), 'cpu_affinity': sorted(os.sched_getaffinity(0)),
        'measured_at_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),
        'commands': commands, 'observations': observations, 'summary': summary,
        'dependency_sha256': before, 'dependency_sha256_after': after,
        'reports': reports, 'outputs_identical': equality, 'inputs_unchanged': True,
        'seed': 20260908, 'measured_iterations': 15, 'discarded_warmup_iterations': 1,
        'runner_sha256': benchmark.digest(Path(__file__)),
        'limitations': ['Warm filesystem cache; subprocess startup and output capture included.',
            'CPU affinity pinned, shared host load and CPU frequency uncontrolled.',
            'Peak RSS includes allocator metadata; allocation counts are not measured.',
            'These binaries predate the later atomic/MMX and parser-budget changes.']}
    (OUT/'evidence.json').write_text(json.dumps(data,indent=2)+'\n')
    print(json.dumps(summary,indent=2))
