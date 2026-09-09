from pathlib import Path
import concurrent.futures,json,subprocess
expressions=['(7 ?: 9)==7','(0 ?: 9)==9','(-1 ?: 2u)<0','(0 ?: -1)<0','(1 ?: (1/0))==1','0 ?: (1/0)','0 && (0 ?: (1/0))','0 ?: 0 ?: 3','1 + 2 ?: 0','1 ?:','1 ? : 2 : 3','1 ?: (2.0)']
rowspec=[(cc,mode,expr)for cc in ['gcc','clang']for mode in ['c90','gnu90','c99','gnu99','c11','gnu11','c17','gnu17']for expr in expressions]
def run(row):
 cc,mode,expr=row;source=f'#if {expr}\nyes\n#else\nno\n#endif\n';command=[cc,'-std='+mode,'-E','-P','-x','c','-'];r=subprocess.run(command,input=source,capture_output=True,text=True,timeout=30);return dict(compiler=cc,mode=mode,expression=expr,source=source,command=command,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr)
with concurrent.futures.ThreadPoolExecutor(max_workers=4)as pool:rows=list(pool.map(run,rowspec))
Path('/home/dev-user/.cache/toucan/omitted-conditional-probes/preprocessor.json').write_text(json.dumps(rows,indent=2)+'\n')
for cc in ['gcc','clang']:print(cc,[(r['expression'],r['exit_code'],r['stdout'].strip())for r in rows if r['compiler']==cc and r['mode']=='gnu11'])
