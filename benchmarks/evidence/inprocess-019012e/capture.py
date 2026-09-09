from pathlib import Path
import hashlib,json,os,platform,random,statistics,subprocess,time
cache=Path('/home/dev-user/.cache/toucan');out=cache/'inprocess-musl-integration';out.mkdir(exist_ok=True);binary=cache/'inprocess-benchmark-target/release/toucan-inprocess-benchmark';randomizer=random.Random(20260908)
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
report={'status':'failed','qualification':'Repeated library calls in one process, one discarded warmup per engine/process, three randomized process pairs with five measured calls each. Shared host, native x86-64 GNU Linux, system allocator. Configuration, parsing and string emission included; CLI startup and first libclang initialization excluded from measured calls.','affinity':sorted(os.sched_getaffinity(0)),'platform':platform.platform(),'projects':{},'binary_sha256':sha(binary)}
try:
 for project in ['zlib','sqlite','zstd','libgit2']:
  original=json.loads((cache/f'benchmarks-bb6a401/{project}.json').read_text());cmd=original['commands']['toucan'];bcmd=original['commands']['bindgen'];request={'header':cmd[2],'target':cmd[cmd.index('--target')+1],'sysroot':cmd[cmd.index('--sysroot')+1],'include_dirs':[cmd[i+1] for i,a in enumerate(cmd) if a=='-I'],'allowlist':[cmd[i+1] for i,a in enumerate(cmd) if a=='--allowlist'],'bindgen_allowlist':[bcmd[i+1] for i,a in enumerate(bcmd) if a=='--allowlist-type']};request_path=out/f'{project}.request.json';request_path.write_text(json.dumps(request,indent=2)+'\n');dependencies={p:sha(p) for p in original['dependency_sha256']};rows=[]
  for pair in range(3):
   engines=['toucan','bindgen'];randomizer.shuffle(engines)
   for position,engine in enumerate(engines):
    generated=out/f'{project}-{engine}-{pair}.rs';command=[str(binary),engine,str(request_path),'5',str(generated)];p=subprocess.run(command,text=True,capture_output=True,check=True,timeout=120);row=json.loads(p.stdout);row.update(pair=pair,order_in_pair=position,command=command,stderr=p.stderr,sha256=sha(generated));rows.append(row)
  for engine in ['toucan','bindgen']:
   hashes={r['sha256'] for r in rows if r['engine']==engine};assert len(hashes)==1
   # Compare complete output with the existing CLI correctness artifact.
   if engine=='toucan': expected=sha(cache/f'feature-catalog-probes/{project}-bindings.rs')
   else: expected=original['observations'][engine][0]['output_sha256']
   assert hashes=={expected},(project,engine,hashes,expected)
  assert dependencies=={p:sha(p) for p in dependencies}
  medians={engine:statistics.median(v for r in rows if r['engine']==engine for v in r['samples_ms']) for engine in ['toucan','bindgen']};report['projects'][project]={'request':request,'rows':rows,'dependency_sha256':dependencies,'median_ms':medians,'bindgen_over_toucan':medians['bindgen']/medians['toucan']};print(project,medians,report['projects'][project]['bindgen_over_toucan'],flush=True)
 assert sha(binary)==report['binary_sha256'];report['status']='passed'
finally:(out/'evidence.json').write_text(json.dumps(report,indent=2)+'\n')
