#!/usr/bin/env python3
"""Rehash the indexed command-hook proof; retained checks require local artifacts."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--master',type=Path,default=Path(__file__).with_name('master.json'));p.add_argument('--retained',action='store_true');a=p.parse_args();m=json.loads(a.master.read_bytes());repo=a.master.resolve().parents[3];count=0
for item in m['artifacts']:
 if item['storage']=='retained'and not a.retained:continue
 q=Path(item['path']);q=q if q.is_absolute()else repo/q
 assert q.stat().st_size==item['bytes']and hashlib.file_digest(q.open('rb'),'sha256').hexdigest()==item['sha256'],str(q);count+=1
print(json.dumps({'passed':True,'indexed_hashes_checked':count,'retained':a.retained}))
