#!/usr/bin/env python3
"""Encode original native RGBA losslessly and independently compare every channel."""
import argparse, hashlib, json, struct, subprocess, zlib
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('directory',type=Path);a=p.parse_args();root=a.directory
pam=(root/'native.p7').read_bytes();hdr=b'P7\nWIDTH 1600\nHEIGHT 1000\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n';assert pam.startswith(hdr)
rgba=pam[len(hdr):];assert len(rgba)==1600*1000*4
ppm=(root/'native.p6').read_bytes();ph=b'P6\n1600 1000\n255\n';assert ppm.startswith(ph);rgb=ppm[len(ph):]
assert rgb==bytes(v for i,v in enumerate(rgba) if i%4!=3)
report=json.loads((root/'native-report.json').read_text());assert report['passed']
alpha=rgba[3::4];assert (min(alpha),max(alpha),sum(x!=255 for x in alpha))==(report['alpha_min'],report['alpha_max'],report['nonopaque_pixels'])
def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
scan=b''.join(b'\0'+rgba[y*6400:(y+1)*6400] for y in range(1000))
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1600,1000,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(scan,9))+chunk(b'IEND',b'')
if (root/'native.png').exists():
    assert (root/'native.png').read_bytes()==png, 'existing PNG must already be the exact constructed bytes'
else:
    with (root/'native.png').open('xb') as f:f.write(png);f.flush();__import__('os').fsync(f.fileno())
# Independent installed FFmpeg PNG decoder must recover all original4 channels.
decoded=subprocess.run(['ffmpeg','-v','error','-i',str(root/'native.png'),'-f','rawvideo','-pix_fmt','rgba','pipe:1'],check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=15).stdout
assert decoded==rgba
checks={'passed':True,'width':1600,'height':1000,'rgba_samples_compared':len(rgba),'rgb_samples_compared':len(rgb),'alpha_min':min(alpha),'alpha_max':max(alpha),'nonopaque_pixels':sum(x!=255 for x in alpha),'all_original_RGBA_bytes_equal_to_independently_decoded_PNG':True,'all_original_RGB_bytes_equal_to_P6':True,'rgba_sha256':hashlib.sha256(rgba).hexdigest(),'p7_sha256':hashlib.sha256(pam).hexdigest(),'p6_sha256':hashlib.sha256(ppm).hexdigest(),'png_sha256':hashlib.sha256(png).hexdigest()}
with (root/'native-channels.json').open('x') as f:json.dump(checks,f,indent=2);f.write('\n')
print(json.dumps(checks))
