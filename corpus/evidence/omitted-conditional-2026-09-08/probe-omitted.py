from pathlib import Path
import concurrent.futures,json,subprocess
out=Path('/home/dev-user/.cache/toucan/omitted-conditional-probes')
cases={
'scalar':'int f(int x,int y){return x ?: y;}',
'floating':'double f(float x,double y){return x ?: y;}',
'complex':'double _Complex f(double _Complex x,double y){return x ?: y;}',
'void_fallback':'void f(int x){x ?: (void)0;}',
'void_condition':'void f(void){(void)0 ?: 1;}',
'lvalue_assignment':'void f(int a,int b){(a ?: b)=4;}',
'lvalue_address':'int*f(int a,int b){return &(a ?: b);}',
'pointer':'const int *f(int*x,const int*y){return x ?: y;}',
'pointer_null':'int*f(int*x){return x ?: 0;}',
'null_condition':'int*f(int*x){return 0 ?: x;}',
'array':'int*f(int*y){static int x[2];return x ?: y;}',
'function':'int g(void);int(*f(int(*p)(void)))(void){return g ?: p;}',
'atomic':'int f(_Atomic(int)*p){return *p ?: 3;}',
'volatile':'int f(volatile int*p){return *p ?: 3;}',
'gnu_bitfield':'struct B{unsigned long x:48;}; unsigned long f(struct B b,unsigned long y){return b.x ?: y;}',
'GNU_lvalue_cond':'int f(int x,int y){return (x=3) ?: y;}',
'vector_condition':'typedef int V __attribute__((vector_size(16)));V f(V a,V b){return a ?: b;}',
'record_condition':'struct S{int x;};struct S f(struct S a,struct S b){return a ?: b;}',
'constant_int':'enum{A=7?:9,B=0?:3};_Static_assert(A==7&&B==3,"values");',
'constant_float':'double x=1.5?:2.5;',
'constant_float_zero':'double x=0.0?:2.5;',
'constant_signed_conversion':'_Static_assert((-1 ?: 2u)==4294967295u,"convert");',
'constant_dead_error':'enum{A=4 ?: (1/0)};',
'constant_live_error':'enum{A=0 ?: (1/0)};',
'constant_bad_fallback':'enum{A=1 ?: undeclared};',
'static_address':'int x;int*p=&x ?: 0;',
'static_string':'char*p="hello" ?: 0;',
'static_null':'int x;int*p=(int*)0 ?: &x;',
'static_both_addresses':'int x,y;int*p=&x ?: &y;',
'static_weak_address':'extern int x __attribute__((weak));int y;int*p=&x ?: &y;',
'statement_local_type':'int f(void){return ({typedef int T;T x=2;x;}) ?: 4;}',
'condition_typedef_scope':'int f(void){return sizeof(enum {E=3}) ?: E;}',
'keyword_prefetch':'void f(int*p){(__builtin_prefetch ?: __builtin_prefetch)(p);}',
'variably_modified':'int n;int (*f(int (*p)[n]))[n]{return p ?: 0;}',
}
profiles=[('gcc','x86_64-unknown-linux-gnu',['gcc'])]+[('clang',t,['clang','--target='+t]) for t in ['x86_64-unknown-linux-gnu','aarch64-unknown-linux-gnu','x86_64-pc-windows-msvc']]
inputs=[]
for c,t,cc in profiles:
 for m in ['c90','gnu90','c99','gnu99','c11','gnu11','c17','gnu17']:
  for n,s in cases.items():inputs.append((c,t,m,n,s,cc+['-std='+m,'-x','c','-fsyntax-only','-']))
def run(x):
 c,t,m,n,s,cmd=x;r=subprocess.run(cmd,input=s,capture_output=True,text=True,timeout=30);return dict(compiler=c,target=t,mode=m,case=n,source=s,command=cmd,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr)
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:rows=list(pool.map(run,inputs))
(out/'initial.json').write_text(json.dumps(dict(versions={c:subprocess.check_output([c,'--version'],text=True) for c in ['gcc','clang']},rows=rows),indent=2)+'\n')
for c,t,_ in profiles:
 print(c,t,[(r['case'],r['exit_code']) for r in rows if r['compiler']==c and r['target']==t and r['mode']=='gnu11'])
print('observations',len(rows))
