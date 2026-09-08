from pathlib import Path
import hashlib,itertools,json,subprocess
cache=Path('/home/dev-user/.cache/toucan');out=cache/'literal-query-differential';out.mkdir(exist_ok=True)
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
atoms=['0','1','-1','255U','0x80000000','0xffffffffffffffffULL','9223372036854775808','0.1f','0.0','-0.0','0x1p-149f','0x1p-1074','1.0L',"'['","L'\\u03bb'"]
expressions=list(atoms)+[f'{op}({atom})' for op in ['+','-','~','!'] for atom in atoms]
expressions += [f'({a}) {op} ({b})' for a,b in zip(atoms,atoms[3:]+atoms[:3]) for op in ['+','-','*','/','%','<<','>>','<','==','&','|','^','&&','||']]
expressions += [f'({c}) ? ({a}) : ({b})' for c in ['0','1','1.0'] for a,b in zip(atoms,atoms[1:]+atoms[:1])]
expressions += ['(Count)KNOWN+sizeof(struct Payload)','__alignof__(Aligned)','sizeof(enum Local { LOCAL_MEMBER=1 })','sizeof(enum Local)','LOCAL_MEMBER','__builtin_constant_p(1.0)','(float)__builtin_nan("")','(_Float16)0.5','(double)(1.0Q/3.0Q)','__builtin_complex(1.0,2.0)','1 /* [ aligned target */ + 2']
source='typedef unsigned Count;typedef int Aligned __attribute__((aligned(32)));struct Payload { Count count; int value; };enum { KNOWN=7 };_Noreturn void stop(void);\n'+''.join(f'typedef struct Payload Alias{i};\n' for i in range(64))+''.join(f'#define QUERY{i} ({expression})\n' for i,expression in enumerate(expressions))
header=out/'queries.h';header.write_text(source);rows=[]
targets=['x86_64-unknown-linux-gnu','aarch64-unknown-linux-gnu','x86_64-apple-darwin','aarch64-apple-darwin','x86_64-pc-windows-msvc','x86_64-unknown-linux-musl','aarch64-unknown-linux-musl']
for target in targets:
 for compiler in (['gcc','clang'] if 'linux' in target else ['clang']):
  for mode in ['gnu11','c11']:
   stem=f'{target}-{compiler}-{mode}';values=[]
   for variant in ['before','after']:
    binary=cache/f'macro-evaluation-{variant}/toucan';output=out/f'{stem}-{variant}.rs';report=out/f'{stem}-{variant}.json';command=[str(binary),'bindgen',str(header),'--target',target,'--compiler',compiler,'--std',mode,'--allowlist','QUERY*','--report',str(report),'-o',str(output)];p=subprocess.run(command,text=True,capture_output=True,check=True,timeout=120);d=json.loads(report.read_text());d.pop('timings');values.append((output.read_bytes(),d));rows.append({'target':target,'compiler':compiler,'mode':mode,'variant':variant,'command':command,'output_sha256':sha(output),'report_sha256':sha(report),'stderr':p.stderr})
   assert values[0]==values[1],stem
(out/'evidence.json').write_text(json.dumps({'status':'passed','macro_expressions':len(expressions),'profile_mode_pairs':len(rows)//2,'source_sha256':sha(header),'rows':rows,'report_comparison':'all fields except elapsed timings'},indent=2)+'\n');print(len(expressions),'expressions across',len(rows)//2,'profile/mode pairs',flush=True)
