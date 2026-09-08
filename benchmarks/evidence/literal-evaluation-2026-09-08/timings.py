from pathlib import Path
import hashlib,json,os,platform,random,statistics,subprocess
cache=Path('/home/dev-user/.cache/toucan');out=cache/'literal-evaluation-paired';out.mkdir(exist_ok=False);before=cache/'inprocess-noreturn-baseline/benchmark';after=cache/'macro-evaluation-after/benchmark';rng=random.Random(20260908)
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
report={'status':'failed','parent_commit':'21fc228','baseline_library_commit':'0342e7f','affinity':sorted(os.sched_getaffinity(0)),'platform':platform.platform(),'samples_per_engine_project':15,'random_seed':20260908,'binary_sha256':{'before':sha(before),'after':sha(after)},'projects':{},'qualification':'Three randomized process triples per project; five measured library calls after one discarded warmup per process. System allocator, shared host with correctness/build work pinned elsewhere; memory bandwidth and CPU frequency uncontrolled. Before/after are Toucan; bindgen 0.72.1 runs from the candidate comparison executable.'}
try:
 for project in ['zlib','sqlite','zstd','libgit2']:
  request=cache/f'inprocess-noreturn-baseline/results/{project}.request.json';reference=json.loads((cache/f'benchmarks-bb6a401/{project}.json').read_text());deps=reference['dependency_sha256'];assert deps=={p:sha(p) for p in deps};rows=[];report['projects'][project]={'rows':rows,'dependency_sha256':deps};(out/f'{project}.request.json').write_bytes(request.read_bytes())
  for pair in range(3):
   engines=['before','after','bindgen'];rng.shuffle(engines)
   for position,engine in enumerate(engines):
    binary=before if engine=='before' else after; actual='bindgen' if engine=='bindgen' else 'toucan';output=out/f'{project}-{engine}-{pair}.rs';command=[str(binary),actual,str(request),'5',str(output)];p=subprocess.run(command,text=True,capture_output=True,check=True,timeout=120);row=json.loads(p.stdout);assert len(row['samples_ms'])==5;expected={v['output_sha256'] for v in reference['observations'][actual]};assert expected=={sha(output)};row.update(variant=engine,pair=pair,order_in_pair=position,command=command,stderr=p.stderr,output_sha256=sha(output));rows.append(row)
  assert deps=={p:sha(p) for p in deps};medians={engine:statistics.median(v for r in rows if r['variant']==engine for v in r['samples_ms']) for engine in ['before','after','bindgen']};report['projects'][project].update(median_ms=medians,speedup=medians['before']/medians['after'],bindgen_over_after=medians['bindgen']/medians['after']);print(project,medians,flush=True)
 assert report['binary_sha256']=={'before':sha(before),'after':sha(after)};report['status']='passed'
finally:(out/'evidence.json').write_text(json.dumps(report,indent=2)+'\n')
