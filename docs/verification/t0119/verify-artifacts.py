#!/usr/bin/env python3
"""Rehash indexed evidence; optionally verify retained native RGBA/RGB channel parity."""
import argparse,hashlib,json,struct,zlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--master',type=Path,default=Path(__file__).with_name('master.json'));p.add_argument('--retained',action='store_true');a=p.parse_args();m=json.loads(a.master.read_text());repo=a.master.resolve().parents[3]
def resolve(path):
 q=Path(path);return q if q.is_absolute() else repo/q
checked=0
for item in m['artifacts']:
 if item['storage']=='retained' and not a.retained:continue
 f=resolve(item['path']);assert hashlib.sha256(f.read_bytes()).hexdigest()==item['sha256'],str(f);checked+=1
captures=0
if a.retained:
 for item in m['captures']:
  p6=resolve(item['p6']).read_bytes();header,dims,maximum,rgb=p6.split(b'\n',3);w,h=map(int,dims.split());assert header==b'P6' and maximum==b'255' and len(rgb)==w*h*3
  p7=resolve(item['p7']).read_bytes();header,rgba=p7.split(b'ENDHDR\n',1);fields=dict(line.split(b' ',1) for line in header.splitlines()[1:]);assert fields=={b'WIDTH':str(w).encode(),b'HEIGHT':str(h).encode(),b'DEPTH':b'4',b'MAXVAL':b'255',b'TUPLTYPE':b'RGB_ALPHA'};assert len(rgba)==w*h*4
  assert rgba[0::4]==rgb[0::3] and rgba[1::4]==rgb[1::3] and rgba[2::4]==rgb[2::3]
  provenance=json.loads(resolve(item['provenance']).read_text());native=provenance['native'];alpha=rgba[3::4];assert min(alpha)==native['alpha_min'] and max(alpha)==native['alpha_max'] and sum(x!=255 for x in alpha)==native['nonopaque_alpha_pixels'];assert native['full_session_unchanged'] and native['document_status_unchanged'] and native['self_close_after_capture'];assert provenance['process_after_capture']=='absent; self-closed'
  png=resolve(item['png']).read_bytes();assert png[:8]==b'\x89PNG\r\n\x1a\n';pos=8;compressed=b''
  while pos<len(png):
   n=struct.unpack('>I',png[pos:pos+4])[0];kind=png[pos+4:pos+8];payload=png[pos+8:pos+8+n];assert struct.unpack('>I',png[pos+8+n:pos+12+n])[0]==zlib.crc32(kind+payload)&0xffffffff
   if kind==b'IHDR':assert struct.unpack('>IIBBBBB',payload)==(w,h,8,2,0,0,0)
   if kind==b'IDAT':compressed+=payload
   pos+=12+n
  raw=zlib.decompress(compressed);stride=w*3+1;assert len(raw)==stride*h and all(raw[y*stride]==0 for y in range(h));assert b''.join(raw[y*stride+1:(y+1)*stride] for y in range(h))==rgb;captures+=1
print(json.dumps({'passed':True,'indexed_hashes_checked':checked,'exact_rgba_rgb_png_captures':captures,'retained':a.retained}))
