"""Losslessly encode the inspection host's native opaque RGB P6 framebuffer."""
import sys,struct,zlib,hashlib,json
from pathlib import Path
source,target=map(Path,sys.argv[1:3])
magic,dimensions,maximum,rgb=source.read_bytes().split(b'\n',3)
assert magic==b'P6' and maximum==b'255'
w,h=map(int,dimensions.split());assert len(rgb)==w*h*3
filtered=b''.join(b'\0'+rgb[y*w*3:(y+1)*w*3] for y in range(h))
def chunk(tag,payload):return struct.pack('>I',len(payload))+tag+payload+struct.pack('>I',zlib.crc32(tag+payload)&0xffffffff)
compressed=zlib.compress(filtered,9)
# Independent decode/readback confirms the exact RGB rows, with no resizing or recoloring.
decoded=zlib.decompress(compressed);assert b''.join(decoded[y*(1+w*3)+1:(y+1)*(1+w*3)] for y in range(h))==rgb
with target.open('xb') as output:output.write(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',compressed)+chunk(b'IEND',b''))
print(json.dumps({'encoding':'lossless RGB P6 to PNG; no image transformations','pixels':[w,h],'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'png_sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'exact_rgb_readback':True},indent=2))
