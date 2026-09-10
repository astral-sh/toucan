#!/usr/bin/env python3
"""Preserve each measured process file by content hash, then remove verified duplicates."""
import gzip,hashlib,io,json,tarfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent
OUT=Path('/home/dev-user/.codex/worktrees/c877/toucan/benchmarks/evidence/handwritten-parser-2026-09-10')
folders=[ROOT/'timing-validated',ROOT/'allocations-validated']
files=[file for folder in folders for file in sorted(folder.iterdir()) if file.is_file()]
blobs={};rows=[]
for file in files:
 data=file.read_bytes();digest=hashlib.sha256(data).hexdigest()
 blobs.setdefault(digest,data)
 rows.append({'path':str(file.relative_to(ROOT)),'bytes':len(data),'sha256':digest,'blob':'blobs/'+digest})
manifest={'format':'sha256-deduplicated-process-files-v1','file_count':len(rows),'unique_blobs':len(blobs),'raw_bytes':sum(row['bytes'] for row in rows),'unique_bytes':sum(len(data) for data in blobs.values()),'files':rows}
archive=OUT/'measurement-artifacts.tar.gz'
with archive.open('wb') as raw:
 with gzip.GzipFile(filename='',mode='wb',fileobj=raw,mtime=0) as compressed:
  with tarfile.open(fileobj=compressed,mode='w') as tar:
   entries={'manifest.json':(json.dumps(manifest,indent=2)+'\n').encode()}
   entries.update({'blobs/'+digest:data for digest,data in blobs.items()})
   for name,data in sorted(entries.items()):
    info=tarfile.TarInfo(name);info.size=len(data);info.mode=0o644;info.mtime=0
    tar.addfile(info,io.BytesIO(data))
verified={}
with tarfile.open(archive,'r:gz') as tar:
 assert json.load(tar.extractfile('manifest.json'))==manifest
 for member in tar.getmembers():
  if member.name=='manifest.json':continue
  data=tar.extractfile(member).read();digest=hashlib.sha256(data).hexdigest()
  assert member.name=='blobs/'+digest
  verified[digest]=len(data)
assert len(verified)==len(blobs)
for row in rows:assert verified[row['sha256']]==row['bytes']
index={**manifest,'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'archive_bytes':archive.stat().st_size,'status':'all archive blobs independently verified'}
(OUT/'measurement-artifacts.json').write_text(json.dumps(index,indent=2)+'\n')
removed=[];retained=[]
for row in rows:
 file=ROOT/row['path']
 if file.name.endswith(('.output','.stdout','.stderr','.rss.txt')):
  assert file.parent in folders
  assert hashlib.sha256(file.read_bytes()).hexdigest()==row['sha256']
  file.unlink();removed.append(row)
 else:retained.append(row['path'])
cleanup={'status':'passed','archive':str(archive),'archive_sha256':index['archive_sha256'],'removed_files':len(removed),'reclaimed_bytes':sum(row['bytes'] for row in removed),'retained_files':retained,'qualification':'Only original files whose exact bytes were reverified in the durable SHA256 blob archive were removed.'}
(ROOT/'measurement-cleanup.json').write_text(json.dumps(cleanup,indent=2)+'\n')
print(json.dumps(cleanup))
