"""Read-only SHA/length and PNG RGB sample verification against this evidence master."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--retained', action='store_true', help='also check unique retained executables and original RGBA/RGB files on this host')
args = parser.parse_args()
root = Path(__file__).parent
master = json.loads((root / 'master.json').read_text())
for name, expected in master['artifacts'].items():
    raw = (root / name).read_bytes()
    assert len(raw) == expected['bytes'], name
    assert hashlib.sha256(raw).hexdigest() == expected['sha256'], name

for capture in master['inspection']['captures']:
    png = (root / capture['png']).read_bytes()
    assert png[:8] == b'\x89PNG\r\n\x1a\n'
    position = 8
    compressed = bytearray()
    width, height = capture['pixels']
    while position < len(png):
        size = struct.unpack_from('>I', png, position)[0]
        kind = png[position + 4:position + 8]
        data = png[position + 8:position + 8 + size]
        crc = struct.unpack_from('>I', png, position + 8 + size)[0]
        assert zlib.crc32(kind + data) & 0xffffffff == crc
        if kind == b'IHDR':
            assert struct.unpack('>IIBBBBB', data) == (width, height, 8, 2, 0, 0, 0)
        if kind == b'IDAT':
            compressed.extend(data)
        position += 12 + size
    rows = zlib.decompress(compressed)
    assert len(rows) == height * (1 + width * 3)
    assert all(rows[y * (1 + width * 3)] == 0 for y in range(height))
    rgb = b''.join(rows[y * (1 + width * 3) + 1:(y + 1) * (1 + width * 3)] for y in range(height))
    assert hashlib.sha256(rgb).hexdigest() == capture['rgb_sha256'], capture['name']
if args.retained:
    for item in master['retained_binaries']:
        assert hashlib.sha256(Path(item['path']).read_bytes()).hexdigest() == item['sha256']
    live = Path(f"/proc/{master['production']['pid']}/exe")
    assert hashlib.sha256(live.read_bytes()).hexdigest() == master['production']['sha256']
    for capture in master['inspection']['captures']:
        assert not Path(f"/proc/{capture['pid']}").exists(), capture['name']
        p6 = Path(capture['retained_p6_path']).read_bytes()
        assert hashlib.sha256(p6).hexdigest() == capture['p6_sha256']
        magic, dims, maximum, rgb = p6.split(b'\n', 3)
        assert magic == b'P6' and maximum == b'255'
        assert list(map(int, dims.split())) == capture['pixels']
        assert hashlib.sha256(rgb).hexdigest() == capture['rgb_sha256']
        p7 = Path(capture['retained_p7_path']).read_bytes()
        assert hashlib.sha256(p7).hexdigest() == capture['p7_sha256']
        header, rgba = p7.split(b'ENDHDR\n', 1)
        assert b'DEPTH 4' in header and b'TUPLTYPE RGB_ALPHA' in header
        assert len(rgba) == capture['pixels'][0] * capture['pixels'][1] * 4
        assert rgba[0::4] == rgb[0::3] and rgba[1::4] == rgb[1::3] and rgba[2::4] == rgb[2::3]
        assert min(rgba[3::4]) == capture['runtime']['alpha_min']
        assert max(rgba[3::4]) == capture['runtime']['alpha_max']
        assert sum(a != 255 for a in rgba[3::4]) == capture['runtime']['nonopaque_alpha_pixels']
        assert hashlib.sha256(Path(capture['binary_path']).read_bytes()).hexdigest() == capture['binary_sha256']
print('Verified', len(master['artifacts']), 'artifact hashes and', len(master['inspection']['captures']), 'exact native PNG RGB readbacks')
