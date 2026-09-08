import hashlib,json,subprocess,shutil
from pathlib import Path
root=Path('/home/dev-user/.cache/toucan/dll-composition-review')
frozen=root/'final-frozen';frozen.mkdir(exist_ok=True)
source=Path('/home/dev-user/.cache/toucan/msvc-declarations-target/debug/toucan')
binary=frozen/'toucan';shutil.copy2(source,binary)
rows=[]
for path in ['calls/evidence.json','scopes/evidence.json','constants/evidence.json','constants/cancellation.json','address-sequences/evidence.json']:
 sourcefile=root/path
 v=json.loads(sourcefile.read_text());cases=v['cases'] if isinstance(v,dict) else v
 for i,old in enumerate(cases):
  file=frozen/f'{sourcefile.parent.name}-{sourcefile.stem}-{i}.c';file.write_text(old['source'])
  cmd=[str(binary),'inspect',str(file),'--target','x86_64-pc-windows-msvc','--checked-code']
  p=subprocess.run(cmd,capture_output=True,text=True)
  native=old['clang']['status'] if 'clang' in old else old.get('status',old.get('native'))
  row={'group':path,'case':i,'native_status':native,'status':p.returncode,'stderr':p.stderr,'command':cmd}
  file.with_suffix('.json').write_text(p.stdout)
  row['match']=(native==0)==(p.returncode==0)
  rows.append(row)
  if not row['match']:print('MISMATCH',row,old['source'])
summary={'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'cases':rows}
(frozen/'evidence.json').write_text(json.dumps(summary,indent=2)+'\n')
print(len(rows),'cases;',sum(not r['match'] for r in rows),'mismatches')
assert all(row['match'] for row in rows)
