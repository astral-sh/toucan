import gzip
import hashlib
import json
from pathlib import Path
import subprocess

root=Path('/home/dev-user/code/oss/toucan')
out=Path('/home/dev-user/.cache/toucan/empty-root-differential')
out.mkdir(exist_ok=False)
binary=root/'target/debug/toucan'
reference=root/'corpus/evidence/c90-integration-2026-09-08/cli-differential.json.gz'
original=json.loads(gzip.decompress(reference.read_bytes()))
markers=['internal compiler error:', 'frontend command failed', 'unable to execute command:', 'please submit a bug report', 'please submit a full bug report', 'llvm error:', 'fatal error: error in backend', 'fatal error: killed signal terminated program', 'the compiler unexpectedly panicked', "thread 'rustc' panicked", "thread 'main' panicked"]
def run(command):
 p=subprocess.run(command,capture_output=True,text=True,timeout=30)
 if p.returncode not in (0,1) or any(s in (p.stdout+p.stderr).lower() for s in markers):
  raise RuntimeError(f'tool failed: {command}: {p.returncode}: {p.stdout}: {p.stderr}')
 return {'command':command,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
rows=[]
for index,row in enumerate(original['rows']):
 path=out/f'{index:03}-{row["name"]}.c';path.write_text(row['source'])
 native=run([row['compiler'],f'-std={row["mode"]}','-fsyntax-only',str(path)])
 command=row['command'].copy();command[0]=str(binary);command[2]=str(path)
 actual=run(command)
 rows.append({'name':row['name'],'compiler':row['compiler'],'mode':row['mode'],'source':row['source'],'source_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'native':native,'toucan':actual,'agrees':(native['exit_code']==0)==(actual['exit_code']==0)})
result={'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),'reference_sha256':hashlib.sha256(reference.read_bytes()).hexdigest(),'versions':{cc:subprocess.check_output([cc,'--version'],text=True) for cc in ('gcc','clang-18')},'rows':rows,'matched':sum(r['agrees'] for r in rows),'total':len(rows)}
(out/'evidence.json').write_text(json.dumps(result,indent=2)+'\n')
print(f"matched {result['matched']}/{result['total']}")
assert result['matched']==result['total']==184, [(r['name'],r['mode'],r['compiler']) for r in rows if not r['agrees']]
