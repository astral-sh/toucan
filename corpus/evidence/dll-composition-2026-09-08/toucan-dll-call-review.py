import hashlib,json,subprocess
from pathlib import Path
root=Path('/home/dev-user/.cache/toucan/dll-composition-review/calls');root.mkdir(exist_ok=True)
toucan=Path('/home/dev-user/.cache/toucan/dll-inline-composition/toucan')
funcs=[('__builtin_malloc','void *','unsigned long long','(1)'),('__builtin_calloc','void *','unsigned long long,unsigned long long','(1,2)'),('__builtin_realloc','void *','void*,unsigned long long','((void*)0,2)'),('__builtin_free','void','void*','((void*)0)'),('__builtin_prefetch','void','const void*,...','((void*)0)')]
forms={'evaluated':'(void)CALL;', 'unevaluated':'(void)sizeof((CALL, 1));', 'vm':'(void)sizeof(int[(CALL, n)]);','vm_nested':'(void)sizeof(sizeof(int[(CALL,n)]));','generic':'(void)_Generic((CALL,1),int:0);','dead':'if(0){(void)CALL;}'}
rows=[]
for i,(name,ret,args,callargs) in enumerate(funcs+[(a.removeprefix('__builtin_'),b,c,d) for a,b,c,d in funcs[:4]]):
 for prefix in ['',f'{ret} {name}({args});',f'{ret} {name}({args}) __asm__("alias_{i}");']:
  for kind,body in forms.items():
   source=prefix+' void g(int n){'+body.replace('CALL',name+callargs)+'} __declspec(dllexport) '+ret+' '+name+'('+args+');\n'
   file=root/f'{len(rows)}.c';file.write_text(source)
   row={'name':name,'prefix':prefix,'kind':kind,'source':source}
   for key,cmd in [('clang',['clang-18','--target=x86_64-pc-windows-msvc','-std=gnu11','-fsyntax-only','-x','c',str(file)]),('toucan',[str(toucan),'check',str(file),'--target','x86_64-pc-windows-msvc'])]:
    p=subprocess.run(cmd,capture_output=True,text=True,timeout=30)
    row[key]={'status':p.returncode,'stderr':p.stderr,'command':cmd}
    if key=='clang' and (p.returncode<0 or 'PLEASE submit a bug report' in p.stderr):raise RuntimeError(row)
   row['match']=(row['clang']['status']==0)==(row['toucan']['status']==0)
   rows.append(row)
(root/'evidence.json').write_text(json.dumps({'toucan_sha256':hashlib.sha256(toucan.read_bytes()).hexdigest(),'cases':rows},indent=2)+'\n')
for row in rows:
 if not row['match']:print(row['name'],repr(row['prefix']),row['kind'],row['clang']['status'],row['toucan']['status'])
print('total',len(rows),'mismatches',sum(not r['match'] for r in rows))
