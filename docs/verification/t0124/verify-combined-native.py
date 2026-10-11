"""Lossless PNG encoding and independent original RGBA/RGB/PNG proof."""
import hashlib,json,os,struct,subprocess,zlib
from pathlib import Path
base=Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0124');captures=[]
def sha(raw):return hashlib.sha256(raw).hexdigest()
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
binary=base/'sidebar-inspection-33981e0532b7';binary_sha=sha(binary.read_bytes())
for mode in ['entries','draft']:
 p6=base/f'{mode}-combined.p6';p7=p6.with_suffix('.p7');png=p6.with_suffix('.png');raw=p6.read_bytes();magic,dims,maximum,rgb=raw.split(b'\n',3);w,h=map(int,dims.split());assert magic==b'P6'and maximum==b'255'and(w,h)==(640,400)and len(rgb)==w*h*3
 original=p7.read_bytes();header,rgba=original.split(b'ENDHDR\n',1);assert b'DEPTH 4'in header and len(rgba)==w*h*4
 assert rgba[0::4]==rgb[0::3]and rgba[1::4]==rgb[1::3]and rgba[2::4]==rgb[2::3]
 assert min(rgba[3::4])==max(rgba[3::4])==255
 rows=b''.join(b'\0'+rgb[y*w*3:(y+1)*w*3]for y in range(h));encoded=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows,9))+chunk(b'IEND',b'')
 with png.open('xb')as f:f.write(encoded);f.flush();os.fsync(f.fileno())
 independent=subprocess.run(['magick',str(png),'-depth','8','RGB:-'],check=True,capture_output=True,timeout=30).stdout;assert independent==rgb
 lines=(base/f'native-{mode}-combined.log').read_text().splitlines();native=next(json.loads(line)for line in lines if line.startswith('{"alpha_max"'))
 assert native['full_session_unchanged']and native['document_status_unchanged']and native['self_close_after_capture']and not native['os_input_claim']and native['ui_passes']>=8
 assert not Path(f"/proc/{native['pid']}").exists()
 for control in native['current_pass_controls']:
  r,c=control['rect'],control['effective_clip'];assert c[0]<=r[0]<=r[2]<=c[2]and c[1]<=r[1]<=r[3]<=c[3]
 captures.append({'mode':mode,'pid':native['pid'],'process_after_capture':'absent; self-closed','png':str(png),'png_sha256':sha(encoded),'p6_sha256':sha(raw),'p7_sha256':sha(original),'rgb_sha256':sha(rgb),'exact_rgb_channels':True,'pixels':[w,h],'native':native})
report={'passed':True,'source':'33981e0532b7d0bba317d577d67d05bd017cb905','binary':str(binary),'binary_sha256':binary_sha,'captures':captures,'png_independent_readback':'ImageMagick -depth 8 RGB:- equals every original P6 RGB sample and P7 RGB channel'}
with (base/'combined-native-verification.json').open('x')as f:json.dump(report,f,indent=2);f.write('\n')
print('PASS two combined native captures; every RGBA/RGB/PNG channel and effective clip verified')
