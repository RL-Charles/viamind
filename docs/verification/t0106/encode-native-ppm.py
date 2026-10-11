"""Lossless P6 RGB to opaque PNG, preserving every renderer readback pixel."""
import hashlib,json,struct,sys,zlib
from pathlib import Path
root=Path(__file__).resolve().parent
reports=[]
for name in sys.argv[1:]:
    data=(root/(name+'.p6')).read_bytes()
    header,dimensions,maximum,rgb=data.split(b'\n',3)
    width,height=map(int,dimensions.split())
    assert header==b'P6' and maximum==b'255' and len(rgb)==width*height*3
    def chunk(kind,payload):return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload)&0xffffffff)
    rows=b''.join(b'\0'+rgb[y*width*3:(y+1)*width*3] for y in range(height))
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows,9))+chunk(b'IEND',b'')
    with (root/(name+'.png')).open('xb') as file:file.write(png)
    reports.append({'name':name,'png':name+'.png','pixels':[width,height],'rgb_sha256':hashlib.sha256(rgb).hexdigest(),'p6_sha256':hashlib.sha256(data).hexdigest()})
print(json.dumps(reports,indent=2))
