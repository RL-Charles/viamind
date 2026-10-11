"""Read-only SHA/length and opaque PNG RGB verification against this evidence master."""
import hashlib
import json
from pathlib import Path
import struct
import zlib

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
print('Verified', len(master['artifacts']), 'artifact hashes and three exact native PNG RGB readbacks')
