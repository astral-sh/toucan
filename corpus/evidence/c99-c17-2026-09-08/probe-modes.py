from pathlib import Path
import concurrent.futures, hashlib, json, subprocess, urllib.request
out = Path('/home/dev-user/.cache/toucan/c99-c17-probes')
MODES = ['c90','gnu90','c99','gnu99','c11','gnu11','c17','gnu17']
PROFILES = [('gcc','x86_64-unknown-linux-gnu',['gcc'])] + [('clang', target,['clang','--target='+target] + (['-mmacosx-version-min=11.0'] if 'apple' in target else [])) for target in ['x86_64-unknown-linux-gnu','aarch64-unknown-linux-gnu','x86_64-unknown-linux-musl','aarch64-unknown-linux-musl','x86_64-apple-darwin','aarch64-apple-darwin','x86_64-pc-windows-msvc']]
CASES = {
 'inline_keyword':'inline int f(void){return 1;}',
 'inline_identifier':'int inline;int f(void){return inline;}',
 'restrict_keyword':'int f(int *restrict p){return *p;}',
 'restrict_identifier':'int restrict;int f(void){return restrict;}',
 'asm_identifier':'int asm;int f(void){return asm;}',
 'typeof_identifier':'typedef int typeof;typeof x;',
 'typeof_keyword':'int x;typeof(x) y;',
 'underscored_keywords':'__inline__ int f(int *__restrict__ p){__typeof__(*p) v=*p;return v;}',
 'utf8_string':'const char *s=u8"hello";',
 'utf16_string':'const unsigned short *s=u"hello";',
 'utf32_string':'const unsigned int *s=U"hello";',
 'utf16_character':"int x=u'a';",
 'utf32_character':"int x=U'a';",
 'wide_string':'const __WCHAR_TYPE__ *s=L"hello";',
 'line_comment':'int value=1; // comment\nint other=2;',
 'for_declaration':'int f(void){int total=0;for(int i=0;i<3;i++)total+=i;return total;}',
 'mixed_declaration':'int f(void){int i=0;i++;int j=i;return j;}',
 'implicit_int':'object;',
 'implicit_function':'int f(void){return unknown(3);}',
 'old_style_implicit':'int f(a) {return a;}',
 'old_style_explicit':'int f(a) int a; {return a;}',
 'alignas':'_Alignas(16) int x;',
 'alignof':'int x=_Alignof(int);',
 'atomic':'_Atomic(int) x;',
 'generic':'int x=_Generic(0,int:1,default:2);',
 'static_assert':'_Static_assert(sizeof(int)==4,"int");',
 'thread_local':'_Thread_local int x;',
 'noreturn':'_Noreturn void f(void);',
 'bool':'_Bool b;',
 'complex':'double _Complex z;',
 'hex_float':'double d=0x1.8p+2;',
 'compound_literal':'struct S{int a;}; struct S s=(struct S){3};',
 'designated_initializer':'struct S{int a,b;}; struct S s={.b=4};',
 'vla':'int f(int n){int a[n];return sizeof(a);}',
 'flexible_array':'struct S{int n;char data[];};',
 'anonymous_struct':'struct S{struct{int x;};};int f(struct S*s){return s->x;}',
 'empty_scalar':'int x={};',
 'digit_separator':"int x=1'000;",
 'binary_literal':'int x=0b1001;',
 'decimal_unsigned_long':'_Static_assert(__builtin_types_compatible_p(__typeof__(9223372036854775808),unsigned long),"unsigned long");',
 'decimal_int128':'_Static_assert(__builtin_types_compatible_p(__typeof__(9223372036854775808),__int128),"int128");',
 'decimal_unsigned_long_long':'_Static_assert(__builtin_types_compatible_p(__typeof__(9223372036854775808),unsigned long long),"unsigned long long");',
 'decimal_long_long':'_Static_assert(__builtin_types_compatible_p(__typeof__(2147483648),long long),"long long");',
 'decimal_long':'_Static_assert(__builtin_types_compatible_p(__typeof__(2147483648),long),"long");',
}
macros = ['__STDC_VERSION__','__STRICT_ANSI__','__STDC_UTF_16__','__STDC_UTF_32__','__GNUC_GNU_INLINE__','__GNUC_STDC_INLINE__','__STDC__','__STDC_HOSTED__','linux','unix']
features = ['c_alignas','c_alignof','c_atomic','c_generic_selections','c_static_assert','c_thread_local','c_variadic_macros','c_unicode_literals','cxx_binary_literals']
query_source='\n'.join(f'{q}_{name} __has_{q}({name})' for q in ['feature','extension'] for name in features)+'\n'
query_source+='namespace __has_attribute(gnu::aligned)\n'
inputs=[]
for c,t,cc in PROFILES:
 for m in MODES:
  for n,s in CASES.items(): inputs.append((c,t,m,n,s,cc+['-std='+m,'-x','c','-fsyntax-only','-']))
  inputs.append((c,t,m,'macros','',cc+['-std='+m,'-x','c','-dM','-E','-']))
  if c=='clang': inputs.append((c,t,m,'queries',query_source.replace('namespace __has_attribute(gnu::aligned)\n',''),cc+['-std='+m,'-x','c','-E','-P','-']))
  else: inputs.append((c,t,m,'namespace','namespace __has_attribute(gnu::aligned)\n',cc+['-std='+m,'-x','c','-E','-P','-']))
def run(x):
 c,t,m,n,s,cmd=x;r=subprocess.run(cmd,input=s,text=True,capture_output=True,timeout=30)
 row=dict(compiler=c,target=t,mode=m,case=n,source=s,command=cmd,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr)
 if n=='macros':
  d={p[1]:p[2] if len(p)>2 else '' for line in r.stdout.splitlines() if (p:=line.split(' ',2)) and p[0]=='#define'};row['selected']={k:d.get(k) for k in macros}
 return row
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool: rows=list(pool.map(run,inputs))
versions={c:subprocess.run([c,'--version'],capture_output=True,text=True).stdout for c in ['gcc','clang']}
(out/'initial.json').write_text(json.dumps(dict(versions=versions,rows=rows),indent=2)+'\n')
for c,t,_ in PROFILES:
 print(c,t)
 for m in ['c99','gnu99','c17','gnu17']:
  group=[r for r in rows if r['compiler']==c and r['target']==t and r['mode']==m]
  print(m,'rejections:',','.join(r['case'] for r in group if r['exit_code']))
  print(next(r['selected'] for r in group if r['case']=='macros'))
print('observations',len(rows))
