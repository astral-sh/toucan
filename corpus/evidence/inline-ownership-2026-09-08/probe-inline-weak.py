from pathlib import Path
import subprocess,json,concurrent.futures
out=Path('/home/dev-user/.cache/toucan/inline-weak-independent');out.mkdir(exist_ok=True)
cases={
'prior_weak':'__attribute__((weak)) int f(void);inline int f(void){return 1;}',
'late_weak':'inline int f(void){return 1;}__attribute__((weak)) int f(void);',
'inline_weak':'inline __attribute__((weak)) int f(void){return 1;}',
'extern_inline_weak':'extern inline __attribute__((weak)) int f(void){return 1;}',
'block_prior':'void g(void){__attribute__((weak)) int f(void);}inline int f(void){return 1;}',
'block_late':'inline int f(void){return 1;}void g(void){__attribute__((weak)) int f(void);}',
'prior_weak_replacement':'__attribute__((weak)) int f(void);extern inline int f(void){return 1;}int f(void){return 2;}',
'written_weak_replacement':'extern inline __attribute__((weak)) int f(void){return 1;}int f(void){return 2;}',
'late_weak_replacement':'extern inline int f(void){return 1;}__attribute__((weak)) int f(void);int f(void){return 2;}',
'late_inline_weak':'inline int f(void){return 1;}inline __attribute__((weak)) int f(void);',
}
profiles=[('gcc','x86_64-linux',['/usr/bin/x86_64-linux-gnu-gcc-13']),('clang','x86_64-linux',['/usr/lib/llvm-18/bin/clang']),('clang','x86_64-windows',['/usr/lib/llvm-18/bin/clang','--target=x86_64-pc-windows-msvc'])];inputs=[]
for n,s in cases.items():
 f=out/(n+'.c');f.write_text(s+'\n')
 for c,t,cc in profiles:
  for mode in ['gnu11','gnu90']:
   cmd=cc+['-std='+mode,'-O0','-S',*(['-emit-llvm']if c=='clang'else[]),'-o','-',str(f)];inputs.append((n,s+'\n',c,t,mode,cmd))
def run(x):
 n,s,c,t,m,cmd=x;r=subprocess.run(cmd,capture_output=True,text=True,timeout=20);obs=[l for l in r.stdout.splitlines()if l.startswith('define')or l.startswith('declare')or'\t.weak'in l or'\t.globl\tf'in l or l=='f:'];return dict(case=n,source=s,compiler=c,target=t,mode=m,command=cmd,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr,observations=obs)
with concurrent.futures.ThreadPoolExecutor(max_workers=4)as ex:rows=list(ex.map(run,inputs))
(out/'evidence.json').write_text(json.dumps(rows,indent=2)+'\n')
for r in rows:print(r['case'],r['compiler'],r['target'],r['mode'],r['exit_code'],r['observations'])
