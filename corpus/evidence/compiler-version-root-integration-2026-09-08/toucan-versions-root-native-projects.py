from pathlib import Path
import hashlib
import json
import subprocess
import time

old = Path('/home/dev-user/.cache/toucan/compiler-version-probes')
out = Path('/home/dev-user/.cache/toucan/versions-root-projects')
rows = []

def run(command):
    start = time.monotonic()
    p = subprocess.run(command, text=True, capture_output=True, timeout=300)
    return dict(command=command, exit_code=p.returncode, stdout=p.stdout,
                stderr=p.stderr, seconds=time.monotonic() - start)

for original in json.loads((old / 'native-projects.json').read_text()):
    row = dict(name=original['name'], source_sha256=original['source_sha256'])
    row['syntax'] = run(original['syntax']['command'])
    row['preprocess'] = run([s.replace(str(old), str(out)) for s in original['preprocess']['command']])
    assert row['syntax']['exit_code'] == row['preprocess']['exit_code'] == 0, row
    command = [s.replace(str(old), str(out)) for s in original['analysis']['command']]
    row['analysis'] = run(command)
    row['analysis']['preprocessed_sha256'] = hashlib.sha256(Path(command[1]).read_bytes()).hexdigest()
    rows.append(row)
    (out / 'native-projects.json').write_text(json.dumps(rows, indent=2) + '\n')
    assert row['analysis']['exit_code'] == 0, row
    print(row['name'], 'native syntax/preprocessing and paired analysis passed', flush=True)
