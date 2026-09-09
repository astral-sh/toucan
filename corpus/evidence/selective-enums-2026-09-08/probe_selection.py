import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent
BINDGEN = Path('/home/dev-user/.cache/toucan/tools/bin/bindgen')
env = dict(os.environ, LIBCLANG_PATH='/usr/lib/llvm-18/lib')
patterns = ['point_conversion_form_t', 'OtherAlias', 'Named', 'NamedAlias',
            'First', 'Second', 'PC_COMPRESSED', 'FIRST_ZERO', 'ANON_ZERO',
            'Nested', 'Holder_Nested', 'Bits', '.*']
rows = []
for index, pattern in enumerate(patterns):
    command = [str(BINDGEN), str(ROOT/'selection.h'), '--rustified-enum', pattern,
               '--no-layout-tests', '--use-core', '--rust-target', '1.70',
               '--no-include-path-detection', '--', '-std=gnu11',
               '--target=x86_64-unknown-linux-gnu']
    result = subprocess.run(command, text=True, capture_output=True, env=env)
    (ROOT/f'selection-{index:02}.rs').write_text(result.stdout)
    rows.append(dict(pattern=pattern, command=command, returncode=result.returncode,
                     stdout=result.stdout, stderr=result.stderr,
                     enums=re.findall(r'pub enum (\w+)', result.stdout)))
(ROOT/'selection-evidence.json').write_text(json.dumps(dict(
    bindgen_sha256=hashlib.sha256(BINDGEN.read_bytes()).hexdigest(),
    bindgen_version=subprocess.check_output([str(BINDGEN),'--version'],text=True).strip(),
    source_sha256=hashlib.sha256((ROOT/'selection.h').read_bytes()).hexdigest(),
    rows=rows), indent=2)+'\n')
print(json.dumps([{k:r[k] for k in ['pattern','returncode','enums']} for r in rows],indent=2))
