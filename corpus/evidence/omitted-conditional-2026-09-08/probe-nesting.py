from pathlib import Path
import json,subprocess,time
root=Path('/home/dev-user/.cache/toucan/omitted-conditional-probes');rows=[]
for n in [8,16,24,48,96,128]:
 value='&object'
 for _ in range(n):value=f'({value} ?: 0)'
 source=f'int object;int*p={value};\n';path=root/f'nesting-{n}.c';path.write_text(source)
 for label,command in [('toucan',[str(root/'head-cli'),'check',str(path),'--std=gnu11','--compiler=gcc','--target=x86_64-unknown-linux-gnu']),('gcc',['gcc','-std=gnu11','-fsyntax-only',str(path)]),('clang',['clang','-std=gnu11','-fsyntax-only',str(path)])]:
  start=time.perf_counter_ns();r=subprocess.run(command,capture_output=True,text=True,timeout=30);elapsed=time.perf_counter_ns()-start;rows.append(dict(nesting=n,source=source,label=label,command=command,exit_code=r.returncode,wall_ns=elapsed,stdout=r.stdout,stderr=r.stderr));print(n,label,r.returncode,round(elapsed/1e6,2),flush=True)
(root/'nesting.json').write_text(json.dumps(rows,indent=2)+'\n')
