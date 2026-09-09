from pathlib import Path
import concurrent.futures,hashlib,importlib.util,json,tempfile
root=Path('/home/dev-user/.codex/worktrees/toucan-inline-integrated')
spec=importlib.util.spec_from_file_location('inline_probe',root/'scripts/probe_inline_ownership.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
fixture=root/'crates/toucan_semantic/tests/fixtures/inline_ownership.json';cases=json.loads(fixture.read_text());inputs=[]
for compiler,target,family in [('gcc',None,'gcc'),('clang','x86_64-unknown-linux-gnu','clang'),('clang','x86_64-pc-windows-msvc','msvc')]:
 for mode in ['c99','gnu99','c17','gnu17']:
  for case in cases:inputs.append((compiler,target,family,mode,case))
def run(x):
 compiler,target,family,mode,case=x
 with tempfile.TemporaryDirectory(prefix='toucan-c99-inline-') as temp:
  record=module.probe(case,compiler,target,mode,Path(temp)/'probe.o')
 record['expected']=case['expected'][family+':gnu11'];record['matches']=record['exit_code'] in(0,1) and record['kind']==record['expected'];return record
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:records=list(pool.map(run,inputs))
document=dict(fixture_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),versions={c:module.capture([c,'--version']) for c in ['gcc','clang']},observations=len(records),matched=sum(r['matches'] for r in records),records=records)
Path('/home/dev-user/.cache/toucan/c99-c17-probes/inline.json').write_text(json.dumps(document,indent=2)+'\n')
print(document['matched'],document['observations'])
for r in records:
 if not r['matches']:print(r['name'],r['mode'],r['target'],r['expected'],r['kind'],r['stderr'])
