import hashlib,json,subprocess
from pathlib import Path
root=Path('/home/dev-user/.cache/toucan/dll-composition-review')
toucan=Path('/home/dev-user/.cache/toucan/dll-inline-composition/toucan')
cases={
'shadow-function': '__declspec(dllimport) __inline__ int f(void){return 1;} void g(int f){ { extern int f(void); (void)f; } } int(*p)(void)=f;',
'shadow-data': '__declspec(dllimport) int x; void g(int x){ { extern int x; static int *p=&x; } }',
'shadow-array': '__declspec(dllimport) int x[4]; void g(int x){ { extern int x[4]; static int *p=x; } }',
'shadow-local-data': '__declspec(dllimport) int x; void g(void){ int x; { extern int x; static int *p=&x; } }',
'shadow-typedef-data': '__declspec(dllimport) int x; void g(void){ typedef int x; { extern int x; static int *p=&x; } }',
'prefetch-use': 'extern int x; void f(void){__builtin_prefetch(&x);} __declspec(dllimport) extern int x;',
'prefetch-sizeof': 'extern int x; void f(void){(void)sizeof(__builtin_prefetch(&x));} __declspec(dllimport) extern int x;',
'prefetch-prototype-lateexport': 'void __builtin_prefetch(const void*,...); void f(void *p){__builtin_prefetch(p);} __declspec(dllexport) void __builtin_prefetch(const void*,...);',
'prefetch-implicit-lateexport': 'void f(void *p){__builtin_prefetch(p);} __declspec(dllexport) void __builtin_prefetch(const void*,...);',
'prefetch-prototype-lateimport': 'void __builtin_prefetch(const void*,...); void f(void *p){__builtin_prefetch(p);} __declspec(dllimport) void __builtin_prefetch(const void*,...);',
'prefetch-import': '__declspec(dllimport) void __builtin_prefetch(const void*,...); void f(void *p){__builtin_prefetch(p);}',
'prefetch-shadow-data': 'void __builtin_prefetch(const void*,...); void f(void){__declspec(dllimport) extern int __builtin_prefetch; static int *p=&__builtin_prefetch;}',
'prefetch-asm-lateexport': 'void __builtin_prefetch(const void*,...) __asm__("pf"); void f(void *p){__builtin_prefetch(p);} __declspec(dllexport) void __builtin_prefetch(const void*,...);',
'allocation-prototype-lateexport': 'void *__builtin_alloca(__SIZE_TYPE__); void f(void){(void)__builtin_alloca(1);} __declspec(dllexport) void *__builtin_alloca(__SIZE_TYPE__);',
'prefetch-shallow-unused': 'extern int x; void f(void){if(0)__builtin_prefetch(&x);} __declspec(dllimport) extern int x;',
}
evidence={'toucan_sha256':hashlib.sha256(toucan.read_bytes()).hexdigest(),'cases':[]}
for name,source in cases.items():
 file=root/f'{name}.c';file.write_text(source+'\n')
 row={'name':name,'source':source}
 for compiler,cmd in [('clang',['clang-18','--target=x86_64-pc-windows-msvc','-std=gnu11','-fsyntax-only','-Xclang','-ast-dump=json',str(file)]),('toucan',[str(toucan),'inspect',str(file),'--target','x86_64-pc-windows-msvc','--checked-code'])]:
  p=subprocess.run(cmd,capture_output=True,text=True,timeout=30)
  (root/f'{name}.{compiler}.stdout').write_text(p.stdout)
  (root/f'{name}.{compiler}.stderr').write_text(p.stderr)
  row[compiler]={'command':cmd,'returncode':p.returncode,'stderr':p.stderr}
  if compiler=='clang' and (p.returncode<0 or 'PLEASE submit a bug report' in p.stderr): raise RuntimeError(f'Compiler crash: {name}')
 row['match']=row['clang']['returncode']==row['toucan']['returncode']
 evidence['cases'].append(row)
 print(name,row['clang']['returncode'],row['toucan']['returncode'])
(root/'review.json').write_text(json.dumps(evidence,indent=2)+'\n')
