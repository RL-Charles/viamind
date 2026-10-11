"""Verify T124 artifact hashes and exact native RGB samples without mutations."""
import argparse,hashlib,json,struct,zlib
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--retained',action='store_true');a=p.parse_args();root=Path(__file__).resolve().parents[3];master=json.loads((Path(__file__).parent/'master.json').read_text())
def sha(raw):return hashlib.sha256(raw).hexdigest()
for item in master['artifacts']:
 if item['storage']=='retained'and not a.retained:continue
 path=root/item['path']if item['storage']=='repository'else Path(item['path']);raw=path.read_bytes();assert len(raw)==item['bytes']and sha(raw)==item['sha256'],str(path)
for item in master['combined_native']['captures']+master['targeted_native']['captures']:
 png=(root/'docs/verification/t0124'/Path(item['png']).name).read_bytes();assert png[:8]==b'\x89PNG\r\n\x1a\n';pos=8;compressed=bytearray();w,h=item['pixels']
 while pos<len(png):
  n=struct.unpack_from('>I',png,pos)[0];kind=png[pos+4:pos+8];data=png[pos+8:pos+8+n];crc=struct.unpack_from('>I',png,pos+8+n)[0];assert zlib.crc32(kind+data)&0xffffffff==crc
  if kind==b'IHDR':assert struct.unpack('>IIBBBBB',data)==(w,h,8,2,0,0,0)
  if kind==b'IDAT':compressed.extend(data)
  pos+=n+12
 rows=zlib.decompress(compressed);assert len(rows)==h*(1+w*3)and all(rows[y*(1+w*3)]==0 for y in range(h));rgb=b''.join(rows[y*(1+w*3)+1:(y+1)*(1+w*3)]for y in range(h))
 if a.retained:
  assert not Path(f"/proc/{item['pid']}").exists()
  p6=Path(item['native']['path']).read_bytes();p7=Path(item['native']['rgba_path']).read_bytes();assert sha(p6)==item['p6_sha256']and sha(p7)==item['p7_sha256']
  assert p6.split(b'\n',3)[3]==rgb;rgba=p7.split(b'ENDHDR\n',1)[1];assert rgba[0::4]==rgb[0::3]and rgba[1::4]==rgb[1::3]and rgba[2::4]==rgb[2::3]and min(rgba[3::4])==max(rgba[3::4])==255
 if 'rgb_sha256'in item:assert sha(rgb)==item['rgb_sha256']
if a.retained:
 production=master['production'];assert sha(Path(f"/proc/{production['pid']}/exe").read_bytes())==production['binary_sha256']
print('Verified',len(master['artifacts']),'indexed artifacts and four exact native PNG RGB readbacks')
