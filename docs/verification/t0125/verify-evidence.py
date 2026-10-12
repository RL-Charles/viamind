#!/usr/bin/env python3
"""Verify immutable repository evidence; optionally include retained local binaries/raw captures."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--repo',type=Path,required=True);p.add_argument('--external',action='store_true');a=p.parse_args()
m=json.loads((a.repo/'docs/verification/t0125/master.json').read_text());count=0
for path,expected in m['artifacts'].items():
 with (a.repo/path).open('rb') as f:actual=hashlib.file_digest(f,'sha256').hexdigest()
 assert actual==expected,(path,expected,actual)
 count+=1
external=0
if a.external:
 for row in m['retained_binaries']+m['external_raw_channels']:
  with Path(row['path']).open('rb') as f:actual=hashlib.file_digest(f,'sha256').hexdigest()
  assert actual==row['sha256'],row
  external+=1
print(json.dumps({'passed':True,'repository_artifacts':count,'external_files':external}))
