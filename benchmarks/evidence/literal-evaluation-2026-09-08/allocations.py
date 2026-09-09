from pathlib import Path
import hashlib,json,re,subprocess
cache=Path('/home/dev-user/.cache/toucan');out=cache/'literal-evaluation-allocation-comparison';out.mkdir(exist_ok=False)
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
report={'status':'failed','projects':{},'baseline_library_commit':'0342e7f','candidate_parent_commit':'21fc228','report_comparison':'Every field except elapsed preprocessing and analysis timings','binaries':{}}
try:
 for variant in ['before','after']:
  root=cache/f'macro-evaluation-{variant}';report['binaries'][variant]={n:sha(root/n) for n in ['toucan','allocations']}
 for project in ['zlib','sqlite','zstd','libgit2']:
  reference=json.loads((cache/f'benchmarks-bb6a401/{project}.json').read_text());deps=reference['dependency_sha256'];assert deps=={p:sha(p) for p in deps};rows={};values=[]
  for variant in ['before','after']:
   root=cache/f'macro-evaluation-{variant}';command=reference['commands']['toucan'].copy();command[0]=str(root/'toucan');rpath=out/f'{project}-{variant}.json';command += ['--report',str(rpath)];p=subprocess.run(command,check=True,capture_output=True,timeout=60);output=p.stdout;(out/f'{project}-{variant}.rs').write_bytes(output);r=json.loads(rpath.read_text());r.pop('timings');values.append((output,r));samples=[];command=[str(root/'allocations')]+reference['commands']['toucan'][2:]
   for _ in range(3):
    p=subprocess.run(command,check=True,capture_output=True,timeout=60);assert p.stdout==output;samples.append(p.stderr.decode().strip())
   assert len(set(samples))==1
   m=re.fullmatch(r'setup (\d+) (\d+) parse (\d+) (\d+) bindings (\d+) (\d+) unit (\d+) profile (\d+)',samples[0]);assert m
   names=['setup_allocations','setup_bytes','parse_allocations','parse_bytes','binding_allocations','binding_bytes','unit_size','profile_size'];counts=dict(zip(names,map(int,m.groups())));rows[variant]={'command':command,'samples':samples,'counts':counts,'output_sha256':hashlib.sha256(output).hexdigest(),'report_sha256':sha(rpath)}
  assert values[0]==values[1],project
  for name in ['setup_allocations','setup_bytes','parse_allocations','parse_bytes','unit_size','profile_size']:assert rows['before']['counts'][name]==rows['after']['counts'][name]
  assert deps=={p:sha(p) for p in deps};report['projects'][project]={'rows':rows,'dependency_sha256':deps};print(project,rows['before']['counts']['binding_allocations'],rows['after']['counts']['binding_allocations'],flush=True)
 for variant,binaries in report['binaries'].items():assert binaries=={n:sha(cache/f'macro-evaluation-{variant}'/n) for n in binaries}
 report['status']='passed'
finally:(out/'evidence.json').write_text(json.dumps(report,indent=2)+'\n')
