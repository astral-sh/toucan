from pathlib import Path
import concurrent.futures,json,subprocess
out=Path('/home/dev-user/.cache/toucan/c99-c17-probes')
profiles=[('gcc','x86_64-unknown-linux-gnu',['gcc']),('clang','x86_64-unknown-linux-gnu',['clang']),('clang','x86_64-pc-windows-msvc',['clang','--target=x86_64-pc-windows-msvc'])]
aliases=['c99','c9x','iso9899:1999','iso9899:199x','gnu99','gnu9x','c11','c1x','iso9899:2011','iso9899:201x','gnu11','gnu1x','c17','c18','iso9899:2017','iso9899:2018','gnu17','gnu18','c94','iso9899:199409','c23','gnu23']
inputs=[]
for c,t,cc in profiles:
 for m in aliases:
  inputs.append((c,t,m,'alias','__STDC_VERSION__\n',cc+['-std='+m,'-x','c','-E','-P','-']))
 for m in ['c99','gnu99','c11','gnu11','c17','gnu17']:
  for n,s in {'trigraph':'int x=1??!2;','implicit_object':'object;','implicit_function':'int f(void){return unknown(3);}','old_style_implicit':'int f(a){return a;}','inline_emit':'inline int f(void){return 1;} int g(void){return f();}','extern_inline_emit':'extern inline int f(void){return 1;} int g(void){return f();}'}.items():
   flags=['-x','c','-E','-P','-'] if n=='trigraph' else ['-x','c','-fsyntax-only','-Werror=implicit-int','-Werror=implicit-function-declaration','-'] if 'implicit' in n else ['-x','c','-O0','-S',*(['-emit-llvm'] if c=='clang' else []),'-o','-','-']
   inputs.append((c,t,m,n,s,cc+['-std='+m]+flags))
def run(x):
 c,t,m,n,s,cmd=x;r=subprocess.run(cmd,input=s,text=True,capture_output=True,timeout=30);return dict(compiler=c,target=t,mode=m,case=n,source=s,command=cmd,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr)
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as p: rows=list(p.map(run,inputs))
(out/'options.json').write_text(json.dumps(rows,indent=2)+'\n')
for r in rows:
 print(r['compiler'],r['target'],r['mode'],r['case'],r['exit_code'],repr(r['stdout']) if r['case'] in ('alias','trigraph') else '')
