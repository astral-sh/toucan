#!/usr/bin/env python3
"""Compare frozen parser, checked semantic, and Builder workloads against baseline."""
import argparse,hashlib,importlib.util,json,os,pathlib,random,statistics,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parent
_module_spec=importlib.util.spec_from_file_location("toucan_parser_comparison",ROOT/"comparison.py")
_module=importlib.util.module_from_spec(_module_spec)
_module_spec.loader.exec_module(_module)
compare_parser,compare_checked,diagnose_json=_module.compare_parser,_module.compare_checked,_module.diagnose_json

def sha(p):return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--candidate',type=pathlib.Path,required=True)
    p.add_argument('--mode',choices=['preflight','timing','allocations'],required=True)
    p.add_argument('--output',type=pathlib.Path,required=True)
    p.add_argument('--preflight',type=pathlib.Path,help='Passed candidate correctness preflight capture.json; required for measurement')
    p.add_argument('--candidate-build',type=pathlib.Path,help='Build manifest binding normal and counter binaries; defaults to candidate directory/build.json')
    p.add_argument('--rounds',type=int,default=7)
    p.add_argument('--iterations',type=int,default=5)
    p.add_argument('--cpu',type=int,default=min(os.sched_getaffinity(0)))
    p.add_argument('--native-work-complete',action='store_true')
    a=p.parse_args()
    if a.mode!='preflight' and not a.native_work_complete:p.error('timing requires --native-work-complete')
    if a.mode!='preflight' and not a.preflight:p.error('measurement requires --preflight CAPTURE.json')
    if a.mode!='preflight' and a.rounds<5:p.error('at least five process pairs required')
    if a.iterations<3:p.error('at least three iterations required')
    if a.cpu not in os.sched_getaffinity(0):p.error('CPU not available')
    spec=json.loads((ROOT/'workloads.json').read_text())
    baseline=json.loads((ROOT/'baseline-build.json').read_text())
    corrections_path=ROOT/'reviewed-checked-span-corrections.json'
    corrections=json.loads(corrections_path.read_text())
    assert corrections['baseline_commit']==baseline['head']
    corrections_sha256=sha(corrections_path)
    baseline_binary=pathlib.Path(baseline['allocation_binary' if a.mode=='allocations' else 'binary'])
    assert sha(baseline_binary)==baseline['allocation_sha256' if a.mode=='allocations' else 'sha256']
    binaries={'baseline':baseline_binary,'candidate':a.candidate.resolve()}
    fingerprints={name:sha(path) for name,path in binaries.items()}
    baseline_rows={r['name']:r for r in json.loads((ROOT/'baseline-preflight/processes.json').read_text())}
    for path,digest in spec['inputs'].items():assert sha(path)==digest,path
    input_digest=hashlib.sha256(json.dumps(spec['inputs'],sort_keys=True).encode()).hexdigest()
    correctness=None
    if a.mode!='preflight':
        correctness=json.loads(a.preflight.read_text())
        assert correctness['status']=='passed' and correctness['mode']=='preflight','correctness preflight did not pass'
        assert correctness['inputs_digest']==input_digest,'preflight used different inputs'
        assert correctness['reviewed_checked_corrections_sha256']==corrections_sha256,'reviewed span policy changed since preflight'
        assert correctness['baseline_commit']==baseline['head']
        assert correctness['binary_sha256']['baseline']==baseline['sha256']
        assert set(correctness['correctness_outputs'])=={w['name'] for w in spec['workloads']}
        if a.mode=='timing':
            assert fingerprints['candidate']==correctness['binary_sha256']['candidate'],'candidate changed since preflight'
        else:
            build=json.loads((a.candidate_build or a.candidate.resolve().parent/'build.json').read_text())
            assert build['status']=='passed'
            assert build['normal']['sha256']==correctness['binary_sha256']['candidate']
            assert build['allocations']['sha256']==fingerprints['candidate']
    a.output.mkdir(exist_ok=False,parents=True)
    rows=[];failures=[]
    result={'status':'failed','mode':a.mode,'random_seed':20260910,'cpu':a.cpu,'binary_sha256':fingerprints,'baseline_commit':baseline['head'],'inputs_digest':input_digest,'rows':rows,'failures':failures,'correctness_outputs':{},'exact_parser_spans':True,'exact_checked_outputs':True,'nonwhitespace_fix_count':0,'reviewed_checked_corrections_sha256':corrections_sha256,'policy':{'builder':'byte equality','builder_reports':'JSON equality after removing timings','checked':'all values exact except proven contiguous source whitespace suffix trims and exact source-pinned reviewed delimiter restorations','parser':'all AST values/counts equal; only proven trailing-whitespace span suffix trims allowed'},'qualification':'Parser spans may omit trailing whitespace; every changed range is retained. Checked qualification separately records whitespace trims and explicitly reviewed closing-delimiter restorations. Builder Rust/reports remain exact. Measurement checks each variant against its own frozen, approved correctness output. Parser/checked timers end at owned-result return; verification, serialization and destruction are excluded. Builder includes rendering. RSS covers the whole process; allocation bytes are cumulative requests.'}
    if correctness:
        result.update(correctness_preflight=str(a.preflight),correctness_preflight_sha256=sha(a.preflight),correctness_outputs=correctness['correctness_outputs'],exact_parser_spans=correctness['exact_parser_spans'],exact_checked_outputs=correctness['exact_checked_outputs'],nonwhitespace_fix_count=correctness['nonwhitespace_fix_count'])
    env=os.environ.copy()
    for key in ['MALLOC_CONF','JEMALLOC_CONF','_RJEM_MALLOC_CONF','MIMALLOC_VERBOSE','MIMALLOC_SHOW_STATS']:env.pop(key,None)
    randomizer=random.Random(20260910)
    try:
        for pair in range(1 if a.mode=='preflight' else a.rounds):
            workloads=list(spec['workloads']);randomizer.shuffle(workloads)
            for workload in workloads:
                names=['candidate'] if a.mode=='preflight' else ['baseline','candidate'];randomizer.shuffle(names)
                for order,name in enumerate(names):
                    label=f"{workload['name']}-{pair}-{name}";output=a.output/(label+'.output');rss=a.output/(label+'.rss.txt')
                    command=['taskset','-c',str(a.cpu),'/usr/bin/time','-v','-o',str(rss),str(binaries[name]),workload['engine'],workload['request'],str(a.iterations),str(output)]
                    start=time.time();process=subprocess.run(command,capture_output=True,text=True,timeout=180,env=env)
                    (a.output/(label+'.stdout')).write_text(process.stdout);(a.output/(label+'.stderr')).write_text(process.stderr)
                    row={'workload':workload['name'],'variant':name,'pair':pair,'order':order,'command':command,'exit_code':process.returncode,'process_seconds':time.time()-start,'output':str(output)};rows.append(row)
                    if process.returncode:
                        failures.append({'workload':workload['name'],'reason':'process failed','exit_code':process.returncode})
                        if a.mode=='preflight':continue
                        process.check_returncode()
                    data=json.loads(process.stdout);assert data['instrumented']==(a.mode=='allocations'),'incorrect instrumentation'
                    output_hash=sha(output)
                    row.update(samples_ms=data['samples_ms'],warmup_ms=data['warmup_ms'],allocations=data['allocations'],output_sha256=output_hash,statistics=data.get('statistics'))
                    row['peak_rss_kib']=int(next(line.split(':',1)[1] for line in rss.read_text().splitlines() if 'Maximum resident set size (kbytes)' in line))
                    if a.mode=='preflight':
                        reference=baseline_rows[workload['name']];reference_file=pathlib.Path(reference['output']);assert sha(reference_file)==reference['output_sha256']
                        comparison={'accepted':True,'kind':'exact_bytes'}
                        if output_hash!=reference['output_sha256']:
                            if workload['engine']=='parser':
                                request=json.loads(pathlib.Path(workload['request']).read_text())
                                comparison=compare_parser(json.loads(reference_file.read_text()),json.loads(output.read_text()),pathlib.Path(request['source']).read_bytes())
                                if not comparison.get('exact_spans',False):result['exact_parser_spans']=False
                            else:
                                comparison={'accepted':False,'kind':'strict_output_mismatch'}
                                if workload['engine']=='checked':
                                    result['exact_checked_outputs']=False
                                    comparison=compare_checked(json.loads(reference_file.read_text()),json.loads(output.read_text()),workload['name'],corrections)
                                    result['nonwhitespace_fix_count']+=comparison.get('restored_delimiter_count',0)
                                try:
                                    diagnosis=diagnose_json(json.loads(reference_file.read_text()),json.loads(output.read_text()))
                                    diagnostic_path=a.output/(label+'.differences.json');diagnostic_path.write_text(json.dumps(diagnosis,indent=2)+'\n');comparison.update(differences=str(diagnostic_path),changed_leaves=diagnosis['changed_leaves'])
                                except json.JSONDecodeError:pass
                        row['comparison']=comparison
                        result['correctness_outputs'][workload['name']]={'baseline':reference['output_sha256'],'candidate':output_hash,'comparison':comparison}
                        if not comparison['accepted']:failures.append({'workload':workload['name'],'reason':comparison['kind']})
                    else:
                        assert output_hash==correctness['correctness_outputs'][workload['name']][name],f'{label}: output differs from its frozen correctness preflight'
                    (a.output/'capture.json').write_text(json.dumps(result,indent=2)+'\n')
        if a.mode=='preflight':
            for project in ['zlib','sqlite','zstd','libgit2']:
                output=a.output/(project+'.report.json');process=subprocess.run([str(binaries['candidate']),'toucan-report',str(ROOT/'requests'/(project+'.json')),'capture',str(output)],capture_output=True,text=True,timeout=180,env=env)
                (a.output/(project+'.report.stdout')).write_text(process.stdout);(a.output/(project+'.report.stderr')).write_text(process.stderr)
                if process.returncode:failures.append({'workload':project+'-report','reason':'process failed'});continue
                report=json.loads(output.read_text());report.pop('timings',None)
                reference=json.loads((ROOT/'baseline-preflight'/(project+'.report.normalized.json')).read_text())
                if report!=reference:
                    failures.append({'workload':project+'-report','reason':'strict report mismatch'})
                    (a.output/(project+'.report.differences.json')).write_text(json.dumps(diagnose_json(reference,report),indent=2)+'\n')
        for path,digest in spec['inputs'].items():assert sha(path)==digest,path
        for name,path in binaries.items():assert sha(path)==fingerprints[name]
        summary={}
        if a.mode!='preflight':
            for workload in spec['workloads']:
                pairs=[]
                for pair in range(a.rounds):
                    both={r['variant']:r for r in rows if r['pair']==pair and r['workload']==workload['name']};assert set(both)=={'baseline','candidate'}
                    b,c=both['baseline'],both['candidate'];item={'pair':pair,'latency_ratio':statistics.median(c['samples_ms'])/statistics.median(b['samples_ms']),'rss_ratio':c['peak_rss_kib']/b['peak_rss_kib']}
                    if a.mode=='allocations':item.update(allocation_calls_ratio=statistics.median(x[0] for x in c['allocations'])/statistics.median(x[0] for x in b['allocations']),requested_bytes_ratio=statistics.median(x[1] for x in c['allocations'])/statistics.median(x[1] for x in b['allocations']))
                    pairs.append(item)
                summary[workload['name']]={'pairs':pairs,'median_ratios':{key:statistics.median(item[key] for item in pairs) for key in pairs[0] if key!='pair'}}
        result.update(status='failed' if failures else 'passed',summary=summary)
    finally:(a.output/'capture.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'status':result['status'],'process_count':len(rows),'exact_parser_spans':result['exact_parser_spans'],'exact_checked_outputs':result['exact_checked_outputs'],'nonwhitespace_fix_count':result['nonwhitespace_fix_count'],'failures':failures,'summary':result.get('summary')}))
    if failures:raise SystemExit(1)
if __name__=='__main__':main()
