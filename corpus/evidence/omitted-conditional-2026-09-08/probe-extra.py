import concurrent.futures,json,subprocess
from pathlib import Path
sources={
'shadow_weak':'extern int w __attribute__((weak)); int f(void){static int w;static int*p=&w ?: 0;return p==&w;}',
'absolute_negative':'int s;int*p=((int*)-1+1) ?: &s;',
'absolute_truncate':'int s;int*p=(int*)((unsigned __int128)1<<64) ?: &s;',
'absolute_member':'struct S{char pad[8];int a;};int s;int*p=&((struct S*)0)->a ?: &s;',
'pointer_converted_null_true':'int x,y;int*p=(1?0:&x) ?: &y;',
'pointer_converted_null_false':'int x,y;int*p=(0?&x:0) ?: &y;',
'vla':'int f(int n,int(*p)[n],int(*q)[n]){return sizeof(*(p ?: q));}',
'vla_increment':'int f(int n,int(*p)[n],int(*q)[n]){return sizeof(*(p++ ?: q));}',
'weak_member_load':'extern struct S{void*p;} w __attribute__((weak));void*p=&w.p ? w.p : 0;',
'call_condition':'int a;int*load(void);int*p=load() ?: &a;',
'narrow_rounding':'_Static_assert((16777217.0f ?: 0.0)==16777216.0,"round");',
'complex_truth':'_Static_assert(__imag__((1.0fi ?: 2.0))==1.0,"imag");',
}
profiles=[('gcc','x86_64-unknown-linux-gnu',['gcc'])]+[('clang',t,['clang','--target='+t]) for t in ['x86_64-unknown-linux-gnu','aarch64-unknown-linux-gnu','x86_64-pc-windows-msvc']]
inputs=[(c,t,m,n,s,cc+['-std='+m,'-x','c','-fsyntax-only','-'])for c,t,cc in profiles for m in ['c90','gnu90','c99','gnu99','c11','gnu11','c17','gnu17'] for n,s in sources.items()]
def run(row):
 c,t,m,n,s,cmd=row;r=subprocess.run(cmd,input=s,capture_output=True,text=True,timeout=30);return dict(compiler=c,target=t,mode=m,name=n,source=s,command=cmd,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr)
with concurrent.futures.ThreadPoolExecutor(max_workers=4)as pool:rows=list(pool.map(run,inputs))
Path('/home/dev-user/.cache/toucan/omitted-conditional-probes/extra.json').write_text(json.dumps(rows,indent=2)+'\n')
for c,t,_ in profiles:print(c,t,[(r['name'],r['exit_code'])for r in rows if r['compiler']==c and r['target']==t and r['mode']=='gnu11'])
