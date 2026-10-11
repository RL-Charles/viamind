#!/usr/bin/env python3
"""Losslessly encode the native P6 framebuffer as PNG without changing RGB bytes."""
import hashlib
import json
import os
from pathlib import Path
import struct
import sys
import zlib

source, output = map(Path, sys.argv[1:])
with source.open('rb') as file:
    assert file.readline(32) == b'P6\n'
    width, height = map(int, file.readline(64).split())
    assert 0 < width * height <= 4_000_000
    assert file.readline(32) == b'255\n'
    rgb = file.read(width * height * 3 + 1)
    assert len(rgb) == width * height * 3
rows = b''.join(b'\0' + rgb[y * width * 3:(y + 1) * width * 3] for y in range(height))
compressed = zlib.compress(rows)
assert zlib.decompress(compressed) == rows


def chunk(kind, data):
    return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))


png = (b'\x89PNG\r\n\x1a\n'
       + chunk(b'IHDR', struct.pack('!2I5B', width, height, 8, 2, 0, 0, 0))
       + chunk(b'IDAT', compressed) + chunk(b'IEND', b''))
with output.open('xb') as file:
    file.write(png)
    file.flush()
    os.fsync(file.fileno())
print(json.dumps({'source': str(source), 'output': str(output), 'pixels': [width, height],
                  'source_ppm_sha256': hashlib.sha256(source.read_bytes()).hexdigest(),
                  'rgb_sha256': hashlib.sha256(rgb).hexdigest(),
                  'png_sha256': hashlib.sha256(png).hexdigest(),
                  'decompressed_rgb_exact': True}, indent=2))
