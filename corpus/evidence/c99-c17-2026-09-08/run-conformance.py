from pathlib import Path
import concurrent.futures,json,subprocess
root=Path('/home/dev-user/.codex/worktrees/toucan-c99-c17');out=Path('/home/dev-user/.cache/toucan/c99-c17-conformance');out.mkdir(exist_ok=True)
def run(x):
 mode,compiler=x;directory=out/(mode+'-'+compiler);cmd=['python',str(root/'scripts/audit_c_testsuite.py'),'--toucan','/home/dev-user/.cache/toucan/c99-c17-probes/head-cli','--cache','/home/dev-user/.cache/toucan/conformance','--offline','--output',str(directory),'--dialect',mode,'--preprocessor',compiler,'--workers','2','--fail-on-strict-difference'];log=out/(mode+'-'+compiler+'.log')
 with log.open('w') as f:r=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,cwd=root)
 print(mode,compiler,r.returncode,flush=True);return dict(mode=mode,preprocessor=compiler,command=cmd,exit_code=r.returncode,log=str(log))
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:rows=list(pool.map(run,[(m,c) for m in ['c99','gnu99','c17','gnu17'] for c in ['gcc','clang']]))
(out/'runs.json').write_text(json.dumps(rows,indent=2)+'\n')
if any(r['exit_code'] for r in rows):raise SystemExit(1)
