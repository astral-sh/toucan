from pathlib import Path
import hashlib,importlib.util,json,os,shutil,subprocess,tarfile,time
root=Path('/home/dev-user/.codex/worktrees/toucan-omitted-conditional');out=Path('/home/dev-user/.cache/toucan/omitted-conditional-focused-replay');out.mkdir();corpus=out/'corpus';corpus.mkdir();artifacts=out/'artifacts';artifacts.mkdir()
spec=importlib.util.spec_from_file_location('runner',root/'scripts/run_fuzz_campaign.py');runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
manifest=runner.source_manifest(root);(out/'source.json').write_text(json.dumps(manifest,indent=2)+'\n')
inputs=[]
for name in ['conditional_addresses.json','omitted_conditional_runtime.json']:
 path=root/'crates/toucan_semantic/tests/fixtures'/name
 for index,case in enumerate(json.loads(path.read_text())):
  for data in runner.seed_profiles(case['source'].encode(),11,modes=8):
   digest=hashlib.sha256(data).hexdigest();(corpus/digest).write_bytes(data);inputs.append(dict(fixture=name,case=index,sha256=digest,profile=sum(data)%11,mode=(sum(data)>>8)&7))
archive=runner.archive_initial_corpus(corpus,out)
binary=Path('/home/dev-user/.cache/toucan/omitted-conditional-fuzz/checked');command=[str(binary),str(corpus),'-runs=0','-timeout=5','-rss_limit_mb=1024','-print_final_stats=1','-seed=9082313',f'-artifact_prefix={artifacts}/'];start=time.monotonic()
with (out/'fuzz.log').open('wb')as log:r=subprocess.run(command,cwd=root,stdout=log,stderr=subprocess.STDOUT,env={**os.environ,'ASAN_OPTIONS':'detect_leaks=0'},timeout=180)
report=dict(command=command,binary_sha256=runner.sha256(binary),exit_code=r.returncode,elapsed_seconds=time.monotonic()-start,inputs=inputs,initial_corpus=archive,initial_archive_sha256=runner.sha256(out/'initial-corpus.tar.gz'),source_manifest_sha256=runner.sha256(out/'source.json'),source_unchanged=manifest==runner.source_manifest(root),artifacts=runner.corpus_manifest(artifacts),log_sha256=runner.sha256(out/'fuzz.log'))
(out/'evidence.json').write_text(json.dumps(report,indent=2)+'\n');print({k:report[k]for k in ['exit_code','elapsed_seconds','source_unchanged','artifacts']});assert not r.returncode and report['source_unchanged'] and not report['artifacts']
