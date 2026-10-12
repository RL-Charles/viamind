"""Encode original native RGB captures and independently verify every retained channel."""
import argparse, hashlib, json, os, struct, subprocess, zlib
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--directory',type=Path,required=True);p.add_argument('--binary',type=Path,required=True);p.add_argument('--source',required=True)
p.add_argument('--completed-binary',type=Path,required=True);p.add_argument('--completed-source',required=True);p.add_argument('--completed-log',required=True)
a=p.parse_args();base=a.directory.resolve()
def sha(data):return hashlib.sha256(data).hexdigest()
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
results=[]
for mode in ['placed','board','completed']:
 p6=base/(mode+'.p6');p7=p6.with_suffix('.p7');png=p6.with_suffix('.png')
 raw=p6.read_bytes();magic,dims,maximum,rgb=raw.split(b'\n',3);w,h=map(int,dims.split())
 assert magic==b'P6'and maximum==b'255'and 0<w*h<=4194304 and len(rgb)==w*h*3
 assert (w,h)==((640,400)if mode=='completed'else(1600,1100))
 original=p7.read_bytes();header,rgba=original.split(b'ENDHDR\n',1)
 fields=dict(line.split(b' ',1)for line in header.splitlines()[1:]if b' 'in line)
 assert fields[b'WIDTH']==str(w).encode()and fields[b'HEIGHT']==str(h).encode()and fields[b'DEPTH']==b'4'and len(rgba)==w*h*4
 assert all(rgba[c::4]==rgb[c::3]for c in range(3))
 rows=b''.join(b'\0'+rgb[y*w*3:(y+1)*w*3]for y in range(h))
 encoded=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows,9))+chunk(b'IEND',b'')
 with png.open('xb')as f:f.write(encoded);f.flush();os.fsync(f.fileno())
 independent=subprocess.run(['magick',str(png),'-depth','8','RGB:-'],capture_output=True,check=True,timeout=30).stdout
 assert independent==rgb
 lines=(base/(a.completed_log if mode=='completed' else 'native-'+mode+'.log')).read_text().splitlines()
 native=next(json.loads(line)for line in lines if line.startswith('{"alpha_max"'))
 assert native['pixels']==[w,h]and native['full_session_unchanged']and native['document_status_unchanged']and native['self_close_after_capture']and not native['os_input_claim']and native['ui_passes']>=8
 assert native['alpha_min']==min(rgba[3::4])and native['alpha_max']==max(rgba[3::4])
 assert not Path('/proc/'+str(native['pid'])).exists()
 for r in native['control_and_field_bounds']:assert 0<=r[0]<=r[2]<=w and 0<=r[1]<=r[3]<=h
 binary=a.completed_binary if mode=='completed' else a.binary
 with binary.open('rb')as f:binary_sha=hashlib.file_digest(f,'sha256').hexdigest()
 results.append({'source':a.completed_source if mode=='completed'else a.source,'binary':str(binary.resolve()),'binary_sha256':binary_sha,'mode':mode,'pixels':[w,h],'png':str(png),'png_sha256':sha(encoded),'p6_sha256':sha(raw),'p7_sha256':sha(original),'rgb_sha256':sha(rgb),'rgba_sha256':sha(rgba),'exact_rgb_channels':True,'process_after_capture':'absent; self-closed','native':native})
report={'passed':True,'captures':results,'independent_decode':'ImageMagick RGB equals every original P6 sample and P7 RGB channel; original alpha retained and indexed'}
with(base/'native-verification.json').open('x')as f:json.dump(report,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
print('PASS three original RGBA / RGB / PNG native captures')
