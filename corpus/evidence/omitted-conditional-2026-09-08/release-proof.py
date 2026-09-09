from pathlib import Path
import hashlib,json,shutil,subprocess
root=Path('/home/dev-user/.cache/toucan/omitted-conditional-probes')
files={'baseline-cli':Path('/home/dev-user/.cache/toucan/c99-c17-probes/head-cli'),'baseline-audit':Path('/home/dev-user/.cache/toucan/c99-c17-probes/head-audit'),'head-cli':Path('/home/dev-user/.cache/toucan/omitted-conditional-target/release/toucan'),'head-audit':Path('/home/dev-user/.cache/toucan/omitted-conditional-target/release/examples/audit_translation_unit')}
manifest={}
for name,path in files.items():
 target=root/name;shutil.copy2(path,target);manifest[name]=dict(path=str(target),sha256=hashlib.sha256(target.read_bytes()).hexdigest(),built_binary=str(path),source_patch_sha256=hashlib.sha256(Path('/tmp/toucan-c99-c17.patch' if name.startswith('baseline') else '/tmp/toucan-omitted-source-proof.patch').read_bytes()).hexdigest())
source='int object;int*p=&object ?: 0;int f(int x,int y){return x ?: y;}\n'
control=root/'release-control.c';control.write_text(source)
rows=[]
for label in ['baseline','head']:
 command=[str(root/(label+'-cli')),'check',str(control),'--std=gnu11','--compiler=gcc','--target=x86_64-unknown-linux-gnu'];r=subprocess.run(command,text=True,capture_output=True);assert (r.returncode==0)==(label=='head');rows.append(dict(command=command,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr))
 request=json.loads(Path('/home/dev-user/.cache/toucan/c99-c17-probes/head-audit-control.request.json').read_text());request.update(input=str(control),language_mode='gnu11',retain_code=True,output=str(root/(label+'-control.unit')))
 request_path=root/(label+'-control.request.json');request_path.write_text(json.dumps(request,indent=2)+'\n');command=[str(root/(label+'-audit')),str(request_path)];r=subprocess.run(command,text=True,capture_output=True);result=json.loads(r.stdout);assert(result['status']=='accepted')==(label=='head');rows.append(dict(command=command,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr))
(root/'release-binaries.json').write_text(json.dumps(manifest,indent=2)+'\n')
(root/'release-control.json').write_text(json.dumps(dict(source=source,rows=rows),indent=2)+'\n')
print(json.dumps(manifest,indent=2))
