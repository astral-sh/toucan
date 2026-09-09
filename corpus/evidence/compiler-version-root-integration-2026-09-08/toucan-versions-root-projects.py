from pathlib import Path
import hashlib
import json
import subprocess
import time

root = Path('/home/dev-user/code/oss/toucan')
old = Path('/home/dev-user/.cache/toucan/compiler-version-probes')
out = Path('/home/dev-user/.cache/toucan/versions-root-projects')
out.mkdir(exist_ok=True)
binary = root / 'target/debug/toucan'

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def run(command):
    start = time.monotonic()
    result = subprocess.run(command, capture_output=True, text=True, timeout=300)
    return dict(command=command, exit_code=result.returncode, stdout=result.stdout,
                stderr=result.stderr, seconds=time.monotonic() - start)

source = (old / 'projects-runner.rs').read_text().replace(
    '/home/dev-user/.cache/toucan/compiler-version-source/fuzz/fuzz_targets/checked_invariants.rs',
    str(root / 'fuzz/fuzz_targets/checked_invariants.rs'))
src = out / 'projects-runner.rs'
src.write_text(source)
library = max((root / 'target/debug/deps').glob('libtoucan-*.rlib'), key=lambda p: p.stat().st_mtime_ns)
runner = out / 'projects-runner'
build = ['rustc', '--edition=2024', '-O', str(src), '-L', 'dependency=' + str(library.parent),
         '--extern', 'toucan=' + str(library), '-o', str(runner)]
subprocess.run(build, check=True)
report = dict(build=build, source_sha256=sha(src), binary_sha256=sha(binary),
              runner_sha256=sha(runner), headers=[], translation_units=[], musl=[])

for original in json.loads((old / 'headers.json').read_text()):
    command = [str(binary)] + [value.replace(str(old), str(out)) for value in original['command'][1:]]
    row = run(command)
    (out / (original['name'] + '.rs')).write_text(row['stdout'])
    row['name'] = original['name']
    row['source_sha256'] = sha(Path(command[2]))
    row['output_sha256'] = hashlib.sha256(row['stdout'].encode()).hexdigest()
    row['unchanged_output'] = row['output_sha256'] == original['stdout_sha256']
    report['headers'].append(row)
    assert row['exit_code'] == 0 and row['unchanged_output'], row
    print('header', row['name'], 'passed', flush=True)

for original in json.loads((old / 'translation-units.json').read_text())['rows']:
    command = [str(runner)] + original['command'][1:]
    command[3] = str(out / (original['name'] + '.i'))
    row = run(command)
    row.update(name=original['name'], source_sha256=sha(Path(command[1])))
    report['translation_units'].append(row)
    assert row['exit_code'] == 0, row
    row['preprocessed_sha256'] = sha(Path(command[3]))
    print('translation unit', row['name'], row['stdout'].strip(), flush=True)

for original in json.loads((old / 'musl.json').read_text()):
    command = [str(binary)] + [value.replace(str(old), str(out)) for value in original['command'][1:]]
    row = run(command)
    row.update(name=original['name'], source_sha256=sha(Path(command[2])))
    row['output_sha256'] = sha(Path(command[command.index('-o') + 1]))
    row['unchanged_output'] = row['output_sha256'] == original['sha256']
    report['musl'].append(row)
    assert row['exit_code'] == 0 and row['unchanged_output'], row
    print('musl', row['name'], 'passed', flush=True)

report['status'] = 'passed'
(out / 'evidence.json').write_text(json.dumps(report, indent=2) + '\n')
