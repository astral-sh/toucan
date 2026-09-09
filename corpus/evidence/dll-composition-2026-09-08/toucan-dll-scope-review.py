import json,subprocess
from pathlib import Path
root=Path('/home/dev-user/.cache/toucan/dll-composition-review/scopes');root.mkdir(exist_ok=True)
shadows={'parameter':('void g(int x){','}'),'local':('void g(void){int x;','}'),'typedef':('void g(void){typedef int x;','}')}
cases=[]
for cls in ['dllimport','dllexport']:
 for name,(start,end) in shadows.items():
  for kind,decl,init in [('object','int x','&x'),('array','int x[4]','x')]:
   cases.append((f'{cls}-{name}-{kind}',f'__declspec({cls}) {decl}; {start} {{extern {decl};static int*p={init};}} {end}'))
  cases.append((f'{cls}-{name}-function',f'__declspec({cls}) __inline__ int x(void){{return 1;}} {start} {{extern int x(void);}} {end} int(*p)(void)=x;'))
cases += [
('block-first-import','void a(void){__declspec(dllimport) extern int x;} void b(int x){{extern int x;static int*p=&x;}}'),
('hidden-outer-import','extern int x;void a(void){ {__declspec(dllimport) extern int x;{int x;{extern int x;static int*p=&x;}}}}'),
('hidden-outer-export','__declspec(dllimport) extern int x;void a(void){ {__declspec(dllexport) extern int x;{int x;{extern int x;static int*p=&x;}}}}'),
('restore-file-import','__declspec(dllimport) extern int x;void a(void){{__declspec(dllexport) extern int x;}{int x;{extern int x;static int*p=&x;}}}'),
('restore-file-plain','extern int x;void a(void){{__declspec(dllimport) extern int x;}{int x;{extern int x;static int*p=&x;}}}'),
('shadow-remains-local','__declspec(dllimport) extern int x;void a(void){static int x;static int*p=&x;}'),
]
rows=[]
for name,source in cases:
 file=root/f'{name}.c';file.write_text(source+'\n')
 p=subprocess.run(['clang-18','--target=x86_64-pc-windows-msvc','-std=gnu11','-fsyntax-only','-Xclang','-ast-dump=json',str(file)],capture_output=True,text=True)
 (root/f'{name}.ast.json').write_text(p.stdout)
 rows.append({'name':name,'source':source,'status':p.returncode,'stderr':p.stderr})
 print(name,p.returncode)
 if p.returncode<0 or 'PLEASE submit a bug report' in p.stderr:raise RuntimeError(name)
(root/'evidence.json').write_text(json.dumps(rows,indent=2)+'\n')
