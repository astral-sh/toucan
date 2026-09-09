import itertools,json,subprocess
from pathlib import Path
root=Path('/home/dev-user/.cache/toucan/dll-composition-review/address-sequences');root.mkdir(exist_ok=True)
rows=[]
for attributes in itertools.product(['','__declspec(dllimport)','__declspec(dllexport)'],repeat=3):
 for blocks in itertools.product([False,True],repeat=2):
  parts=[]
  for i,attr in enumerate(attributes):
   decl=f'{attr} extern int x;'
   parts.append(f'void f{i}(void){{{decl}}}' if i<2 and blocks[i] else decl)
  source=''.join(parts)+'static int*p=&x;';file=root/f'{len(rows)}.c';file.write_text(source)
  cmd=['clang-18','--target=x86_64-pc-windows-msvc','-std=gnu11','-fsyntax-only','-Xclang','-ast-dump=json',str(file)]
  p=subprocess.run(cmd,capture_output=True,text=True)
  (file.with_suffix('.ast.json')).write_text(p.stdout)
  rows.append({'source':source,'status':p.returncode,'stderr':p.stderr,'command':cmd})
(root/'evidence.json').write_text(json.dumps(rows,indent=2)+'\n')
print(len(rows),'native declaration sequences')
