"""Retain original Screenshot RGBA; encode identical RGB samples as review PNG."""
import hashlib,json,os,struct,sys,zlib
from pathlib import Path
root=Path(__file__).resolve().parent
reports=[]
for name in sys.argv[1:]:
    data=(root/(name+'.p6')).read_bytes()
    header,dimensions,maximum,rgb=data.split(b'\n',3)
    width,height=map(int,dimensions.split())
    assert header==b'P6' and maximum==b'255' and len(rgb)==width*height*3
    original=(root/(name+'.p7')).read_bytes();p7header,rgba=original.split(b'ENDHDR\n',1)
    fields=dict(line.split(b' ',1)for line in p7header.splitlines()[1:])
    assert fields=={b'WIDTH':str(width).encode(),b'HEIGHT':str(height).encode(),b'DEPTH':b'4',b'MAXVAL':b'255',b'TUPLTYPE':b'RGB_ALPHA'}
    assert len(rgba)==width*height*4
    assert all(rgb[i*3:i*3+3]==rgba[i*4:i*4+3]for i in range(width*height))
    alpha=rgba[3::4];runtime=json.loads((root/(name+'-provenance.json')).read_text())['native']
    assert runtime['alpha_min']==min(alpha)and runtime['alpha_max']==max(alpha)and runtime['nonopaque_alpha_pixels']==sum(a!=255 for a in alpha)
    def chunk(kind,payload):return struct.pack('>I',len(payload))+kind+payload+struct.pack('>I',zlib.crc32(kind+payload)&0xffffffff)
    rows=b''.join(b'\0'+rgb[y*width*3:(y+1)*width*3]for y in range(height))
    png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows,9))+chunk(b'IEND',b'')
    with(root/(name+'.png')).open('xb')as file:file.write(png);file.flush();os.fsync(file.fileno())
    pos=8;compressed=b''
    while pos<len(png):
        size=struct.unpack('>I',png[pos:pos+4])[0];kind=png[pos+4:pos+8];payload=png[pos+8:pos+8+size]
        assert struct.unpack('>I',png[pos+8+size:pos+12+size])[0]==zlib.crc32(kind+payload)&0xffffffff
        if kind==b'IDAT':compressed+=payload
        pos+=12+size
    decoded=zlib.decompress(compressed);stride=width*3+1
    assert len(decoded)==height*stride and all(decoded[y*stride]==0 for y in range(height))
    assert b''.join(decoded[y*stride+1:(y+1)*stride]for y in range(height))==rgb
    sha=lambda value:hashlib.sha256(value).hexdigest()
    reports.append({'name':name,'png':name+'.png','pixels':[width,height],'rgb_sha256':sha(rgb),'p6_path':str(root/(name+'.p6')),'p6_sha256':sha(data),'p7_path':str(root/(name+'.p7')),'p7_sha256':sha(original),'rgba_sha256':sha(rgba),'rgba_bytes':len(rgba),'alpha_min':min(alpha),'alpha_max':max(alpha),'nonopaque_alpha_pixels':sum(a!=255 for a in alpha),'all_rgba_bytes_retained':True,'every_rgb_sample_matches_png':True,'scope':'Original Screenshot Color32 RGBA channels retained; PNG omits alpha and preserves the identical RGB samples. No opacity/compositor/OS-input claim.'})
print(json.dumps(reports,indent=2))
