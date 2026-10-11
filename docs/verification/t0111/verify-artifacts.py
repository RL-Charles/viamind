"""Verify durable hashes and every retained native RGBA/RGB/review PNG sample."""
import hashlib,json,struct,zlib
from pathlib import Path
repo=Path(__file__).resolve().parents[3]
master=json.loads((Path(__file__).parent/'master.json').read_text())
sha=lambda data:hashlib.sha256(data).hexdigest()
for item in master['artifacts']:
    data=(repo/item['path']).read_bytes()
    assert len(data)==item['bytes']and sha(data)==item['sha256'],item['path']
for capture in master['native_captures']:
    p6=Path(capture['p6_path']).read_bytes();p7=Path(capture['p7_path']).read_bytes()
    assert sha(p6)==capture['p6_sha256']and sha(p7)==capture['p7_sha256']
    _,dimensions,maximum,rgb=p6.split(b'\n',3);width,height=map(int,dimensions.split())
    assert maximum==b'255'and[width,height]==capture['pixels']and len(rgb)==width*height*3
    header,rgba=p7.split(b'ENDHDR\n',1)
    assert b'DEPTH 4\n'in header and len(rgba)==capture['rgba_bytes']==width*height*4
    assert sha(rgba)==capture['rgba_sha256']and sha(rgb)==capture['rgb_sha256']
    assert all(rgba[i*4:i*4+3]==rgb[i*3:i*3+3]for i in range(width*height))
    alpha=rgba[3::4]
    assert min(alpha)==capture['alpha_min']and max(alpha)==capture['alpha_max']and sum(a!=255 for a in alpha)==capture['nonopaque_alpha_pixels']
    png=(Path(__file__).parent/capture['png']).read_bytes();assert png[:8]==b'\x89PNG\r\n\x1a\n'
    pos=8;compressed=b''
    while pos<len(png):
        size=struct.unpack('>I',png[pos:pos+4])[0];kind=png[pos+4:pos+8];data=png[pos+8:pos+8+size]
        assert struct.unpack('>I',png[pos+8+size:pos+12+size])[0]==zlib.crc32(kind+data)&0xffffffff
        if kind==b'IHDR':assert struct.unpack('>IIBBBBB',data)==(width,height,8,2,0,0,0)
        if kind==b'IDAT':compressed+=data
        pos+=12+size
    decoded=zlib.decompress(compressed);stride=width*3+1
    assert len(decoded)==height*stride and all(decoded[y*stride]==0 for y in range(height))
    assert b''.join(decoded[y*stride+1:(y+1)*stride]for y in range(height))==rgb
for section in ['production','inspection_refresh']:
    assert sha(Path(master[section]['path']).read_bytes())==master[section]['sha256']
print(json.dumps({'passed':True,'durable_hashes':len(master['artifacts']),'original_rgba_and_every_rgb_png_sample':len(master['native_captures'])}))
