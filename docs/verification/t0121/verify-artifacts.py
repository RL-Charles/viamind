"""Read-only verification of T121 indexed artifacts and every native PNG RGB sample."""
import argparse
import hashlib
import json
import struct
import zlib
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--retained', action='store_true', help='also verify local executables and original P6/P7 channels')
a = p.parse_args()
root = Path(__file__).resolve().parents[3]
base = Path(__file__).parent
master = json.loads((base / 'master.json').read_text())


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


for item in master['artifacts']:
    if item['storage'] == 'retained' and not a.retained:
        continue
    path = root / item['path'] if item['storage'] == 'repository' else Path(item['path'])
    raw = path.read_bytes()
    assert len(raw) == item['bytes'] and sha(raw) == item['sha256'], str(path)
for item in master['native']['captures']:
    png = (base / 'final' / item['png']).read_bytes()
    assert png[:8] == b'\x89PNG\r\n\x1a\n' and sha(png) == item['png_sha256']
    pos, compressed, (w, h) = 8, bytearray(), item['pixels']
    assert 0 < w * h <= 4194304 and item['exited']
    while pos < len(png):
        n = struct.unpack_from('>I', png, pos)[0]
        kind, data = png[pos + 4:pos + 8], png[pos + 8:pos + 8 + n]
        assert zlib.crc32(kind + data) & 0xffffffff == struct.unpack_from('>I', png, pos + 8 + n)[0]
        if kind == b'IHDR':
            assert struct.unpack('>IIBBBBB', data) == (w, h, 8, 2, 0, 0, 0)
        if kind == b'IDAT':
            compressed.extend(data)
        pos += n + 12
    rows = zlib.decompress(compressed)
    assert len(rows) == h * (1 + w * 3) and all(rows[y * (1 + w * 3)] == 0 for y in range(h))
    rgb = b''.join(rows[y * (1 + w * 3) + 1:(y + 1) * (1 + w * 3)] for y in range(h))
    assert sha(rgb) == item['rgb_sha256']
    assert item['helper']['full_session_unchanged'] and item['helper']['document_status_unchanged'] and not item['helper']['os_input_claim']
    if a.retained:
        p6, p7 = Path(item['p6']).read_bytes(), Path(item['p7']).read_bytes()
        assert sha(p6) == item['p6_sha256'] and sha(p7) == item['p7_sha256'] and p6.split(b'\n', 3)[3] == rgb
        rgba = p7.split(b'ENDHDR\n', 1)[1]
        assert sha(rgba) == item['rgba_sha256'] and all(rgba[c::4] == rgb[c::3] for c in range(3))
        assert min(rgba[3::4]) == item['alpha_min'] and max(rgba[3::4]) == item['alpha_max']
if a.retained:
    for b in master['binaries'].values():
        assert sha(Path(b['path']).read_bytes()) == b['sha256']
for name, r in master['actual_checks'].items():
    assert r['passed'] and r['binary_sha256'] == master['binaries']['application']['sha256'], name
print('PASS', len(master['artifacts']), 'indexed hashes and', len(master['native']['captures']), 'exact native PNG RGB readbacks')
