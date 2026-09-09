from pathlib import Path
import hashlib,json,os,random,re,statistics,subprocess,time
affinity=sorted(os.sched_getaffinity(0));os.sched_setaffinity(0,{affinity[-1]})
root=Path('/home/dev-user/.cache/toucan/omitted-conditional-probes');out=root/'benchmark';out.mkdir(exist_ok=False)
binaries={'baseline':root/'baseline-audit','candidate':root/'head-audit'}
requests=sorted(Path('/home/dev-user/.cache/toucan/c99-c17-source-audit').glob('*/compiler_preprocessed/normal.request.json'))
manifest={str(path):hashlib.sha256(path.read_bytes()).hexdigest() for path in binaries.values()}
sources={}
cases=[]
for request in requests:
 for retained in [False,True]:
  value=json.loads(request.read_text());source=Path(value['input']);sources[str(source)]=hashlib.sha256(source.read_bytes()).hexdigest()
  value['retain_code']=retained
  label=request.parents[1].name+('-retained' if retained else '-ordinary')
  value['output']=str(out/(label+'.unit'))
  path=out/(label+'.request.json');path.write_text(json.dumps(value,indent=2)+'\n')
  cases.append((label,path,value))
rows=[]
for iteration in range(-1,7):
 shuffled=cases.copy();random.Random(9082312+iteration).shuffle(shuffled)
 for label,path,request in shuffled:
  normalized=None
  for binary in (['baseline','candidate'] if iteration%2==0 else ['candidate','baseline']):
   usage=out/'usage.json'
   command=['/usr/bin/time','-f','{"user_seconds":%U,"system_seconds":%S,"rss_kib":%M}','-o',str(usage),str(binaries[binary]),str(path)]
   start=time.perf_counter_ns();run=subprocess.run(command,text=True,capture_output=True,timeout=180);elapsed=time.perf_counter_ns()-start
   if run.returncode:raise RuntimeError((command,run.stdout,run.stderr))
   result=json.loads(run.stdout);assert result['status']=='accepted',result
   unit=Path(request['output']).read_bytes()
   digest=hashlib.sha256(unit).hexdigest()
   if normalized is None:normalized=digest
   else:assert digest==normalized,(label,binary,'declaration mismatch')
   rows.append(dict(case=label,iteration=iteration,binary=binary,command=command,wall_ns=elapsed,usage=json.loads(usage.read_text()),result=result,normalized_unit_sha256=digest))
  print(iteration,label,flush=True)
summary=[]
for label,_,_ in cases:
 baseline=[row['wall_ns'] for row in rows if row['case']==label and row['binary']=='baseline' and row['iteration']>=0]
 head=[row['wall_ns'] for row in rows if row['case']==label and row['binary']=='candidate' and row['iteration']>=0]
 summary.append(dict(case=label,baseline_median_ns=statistics.median(baseline),candidate_median_ns=statistics.median(head),ratio=statistics.median(head)/statistics.median(baseline),baseline_peak_rss_kib=max(row['usage']['rss_kib'] for row in rows if row['case']==label and row['binary']=='baseline'),candidate_peak_rss_kib=max(row['usage']['rss_kib'] for row in rows if row['case']==label and row['binary']=='candidate')))
assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest for path,digest in sources.items())
assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest()==digest for path,digest in manifest.items())
(root/'benchmark.json').write_text(json.dumps(dict(cpu_affinity=[affinity[-1]], method='Seven measured paired process runs after one warmup; deterministic shuffled case order; alternating binary order; full compiler-preprocessed real-TU audit including preprocessing, analysis, and complete unit serialization. Complete declaration outputs must match byte for byte; no metadata is removed.',binaries=manifest,sources=sources,summary=summary,rows=rows),indent=2)+'\n')
print(json.dumps(summary,indent=2))
