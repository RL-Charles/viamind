#!/usr/bin/env python3
"""Losslessly encode an native P6 RGB review samples as PNG, using only stdlib."""
import hashlib,struct,sys,zlib
from pathlib import Path
for raw in sys.argv[1:]:
    p=Path(raw)
    magic,dims,maxv,rgb=p.read_bytes().split(b'\n',3)
    w,h=map(int,dims.split())
    assert magic==b'P6' and maxv==b'255' and len(rgb)==w*h*3
    rows=b''.join(b'\x00'+rgb[y*w*3:(y+1)*w*3] for y in range(h))
    def chunk(name,data):
        return struct.pack('>I',len(data))+name+data+struct.pack('>I',zlib.crc32(name+data)&0xffffffff)
    compressed=zlib.compress(rows,9)
    decoded=zlib.decompress(compressed)
    assert b''.join(decoded[y*(1+w*3)+1:(y+1)*(1+w*3)] for y in range(h))==rgb
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',compressed)+chunk(b'IEND',b'')
    out=p.with_suffix('.png')
    out.open('xb').write(png)
    print(out,w,h,hashlib.sha256(png).hexdigest())
