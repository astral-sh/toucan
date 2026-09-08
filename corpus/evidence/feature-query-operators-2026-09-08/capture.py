from pathlib import Path
import hashlib,json,re,subprocess
root=Path('/home/dev-user/code/oss/toucan');out=Path('/home/dev-user/.cache/toucan/query-operator-validation');out.mkdir(exist_ok=True)
source=(root/'crates/toucan_preprocessor/tests/feature_queries.rs').read_text()
block=source.split('const CASES:')[1].split('\n];',1)[0]
strings=re.findall(r'\(\s*("(?:\\.|[^"\\])*")\s*,\s*("(?:\\.|[^"\\])*")\s*,?\s*\)',block)
cases=[(json.loads(a),json.loads(b)) for a,b in strings];assert len(cases)==22,len(cases)
driver=out/'driver.rs';driver.write_text(source+'''
fn main() {
    use std::io::Read;
    let args=std::env::args().collect::<Vec<_>>();
    let dialect=if args[1]=="gcc" { QueryDialect::Gnu } else {QueryDialect::Clang};
    let mut c=config(dialect);
    c.feature_queries.as_mut().unwrap().provider=Arc::new(Catalog {dialect,gnu_namespace:args[2]=="gnu11"});
    let mut source=String::new();std::io::stdin().read_to_string(&mut source).unwrap();
    match Preprocessor::new(c).preprocess_str(Path::new("query.h"),&source) {
        Ok(result)=>print!("{}",result.source),
        Err(error)=>{eprintln!("{error}");std::process::exit(1);}
    }
}
''')
lib=max((root/'target/debug/deps').glob('libtoucan_preprocessor-*.rlib'),key=lambda p:p.stat().st_mtime)
cmd=['rustc','--edition=2024','-Adead_code',str(driver),'--extern','toucan_preprocessor='+str(lib),'-L','dependency='+str(root/'target/debug/deps'),'-o',str(out/'driver')]
subprocess.run(cmd,check=True)
ccs=[('gcc-native','gcc',['gcc']),('gcc-arm','gcc',['/home/dev-user/.cache/toucan/aarch64-gcc-13/usr/bin/aarch64-linux-gnu-gcc-13','-B/home/dev-user/.cache/toucan/aarch64-gcc-13/usr/lib/gcc-cross/aarch64-linux-gnu/13'])]
ccs += [('clang-'+target,'clang',['clang','--target='+target]) for target in ['x86_64-unknown-linux-gnu','aarch64-unknown-linux-gnu','x86_64-apple-darwin','aarch64-apple-darwin','x86_64-pc-windows-msvc']]
rows=[]
for profile,dialect,cc in ccs:
 for mode in ('c11','gnu11'):
  for case,text in cases:
   native_cmd=cc+['-E','-P','-x','c','-std='+mode,'-'];ours_cmd=[str(out/'driver'),dialect,mode]
   n=subprocess.run(native_cmd,input=text,capture_output=True,text=True,timeout=10);t=subprocess.run(ours_cmd,input=text,capture_output=True,text=True,timeout=10)
   assert n.returncode in (0,1) and t.returncode in (0,1)
   for marker in ('internal compiler error','please submit a bug report','unable to execute command','frontend command failed'):
    assert marker not in (n.stdout+n.stderr).lower()
   equal=n.returncode==t.returncode and (n.returncode!=0 or ''.join(n.stdout.split())==''.join(t.stdout.split()))
   rows.append({'profile':profile,'mode':mode,'case':case,'source':text,'source_sha256':hashlib.sha256(text.encode()).hexdigest(),'native':{'command':native_cmd,'status':n.returncode,'stdout':n.stdout,'stderr':n.stderr},'toucan':{'command':ours_cmd,'status':t.returncode,'stdout':t.stdout,'stderr':t.stderr},'equal':equal})
   assert equal,rows[-1]
report={'schema_version':1,'scope':'Preprocessor operator grammar, expansion, and override semantics with a finite test catalog; no ABI validation.','source_sha256':hashlib.sha256(source.encode()).hexdigest(),'driver_sha256':hashlib.sha256(driver.read_bytes()).hexdigest(),'driver_binary_sha256':hashlib.sha256((out/'driver').read_bytes()).hexdigest(),'build_command':cmd,'compiler_versions':{name:subprocess.check_output(cc+['--version'],text=True) for name,_,cc in ccs},'rows':rows,'decisions':len(rows),'mismatches':0}
(out/'evidence.json').write_text(json.dumps(report,indent=2)+'\n');print('decisions',len(rows),'mismatches',0)
