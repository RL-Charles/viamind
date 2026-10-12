"""Verify retained hardened diagnostic hashes and every original RGBA/RGB sample."""
import gzip
import hashlib
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parent
master = json.loads((root / "master.json").read_text())
for artifact in master["artifacts"]:
    data = (root / artifact["path"]).read_bytes()
    assert len(data) == artifact["bytes"], artifact["path"]
    assert hashlib.sha256(data).hexdigest() == artifact["sha256"], artifact["path"]
for raw in master["raw_files"]:
    data = gzip.decompress((root / raw["path"]).read_bytes())
    assert len(data) == raw["original_bytes"], raw["path"]
    assert hashlib.sha256(data).hexdigest() == raw["original_sha256"], raw["path"]
for index in range(10):
    name = f"image-{index:02}"
    original = gzip.decompress((root / f"pairs/{name}.p7.gz").read_bytes())
    header, rgba = original.split(b"ENDHDR\n", 1)
    fields = dict(line.split(b" ", 1) for line in header.splitlines()[1:] if b" " in line)
    width, height = int(fields[b"WIDTH"]), int(fields[b"HEIGHT"])
    assert len(rgba) == width * height * 4
    rgb = bytearray(width * height * 3)
    rgb[0::3], rgb[1::3], rgb[2::3] = rgba[0::4], rgba[1::4], rgba[2::4]
    p6 = gzip.decompress((root / f"pairs/{name}.p6.gz").read_bytes())
    assert p6 == f"P6\n{width} {height}\n255\n".encode() + rgb
    for kind, expected, output in [("rgba", rgba, "rgba:-"), ("rgb-review", rgb, "rgb:-")]:
        decoded = subprocess.check_output(["magick", str(root / f"analysis/{name}-{kind}.png"), "-depth", "8", output])
        assert decoded == expected, (name, kind)
print(f"PASS {len(master['artifacts'])} indexed hashes, 20 original raw files, 20 exact PNG sample comparisons")
