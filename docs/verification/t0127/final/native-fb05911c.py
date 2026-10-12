"""Launch each owned T127 inspection wrapper through cua-driver launch_app, wait for self-close, retain receipts."""
import hashlib, json, struct, subprocess, sys, time, zlib
from pathlib import Path
sys.dont_write_bytecode = True
E = Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0127'); s = 'fb05911c'
def sha(b): return hashlib.sha256(b).hexdigest()
def png(w, h, rgb):
    raw = b''.join(b'\0' + rgb[y*w*3:(y+1)*w*3] for y in range(h))
    def chunk(k, d): return struct.pack('>I', len(d)) + k + d + struct.pack('>I', zlib.crc32(k + d) & 0xffffffff)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')
fixture = E / f'inspection-fixture-{s}.viamind.json'; fixture_sha = sha(fixture.read_bytes())
results = []
for mode in sys.argv[1:]:
    wrapper = E / f'launch-{mode}-{s}.sh'
    args = {'launch_path': str(wrapper)}
    t0 = time.time()
    proc = subprocess.run(['cua-driver', 'launch_app', json.dumps(args)], capture_output=True, text=True, timeout=120)
    receipt = {'tool': 'launch_app', 'arguments': args, 'returncode': proc.returncode, 'stdout': proc.stdout, 'stderr': proc.stderr}
    (E / f'native-{mode}-{s}-launch.json').open('x').write(json.dumps(receipt, indent=2) + '\n')
    pid = json.loads(proc.stdout)['pid']
    while Path(f'/proc/{pid}').exists() and time.time() - t0 < 180: time.sleep(0.25)
    exited = not Path(f'/proc/{pid}').exists()
    out = E / f'native-{mode}-{s}'
    item = {'mode': mode, 'pid': pid, 'exited': exited, 'seconds': round(time.time() - t0, 2), 'log': (E / f'launch-{mode}-{s}.sh.log').read_text()}
    if (out / 'native-report.json').exists():
        report = json.loads((out / 'native-report.json').read_text())
        p6 = (out / 'native.p6').read_bytes(); head, rgb = p6.split(b'\n', 3)[:3], p6.split(b'\n', 3)[3]
        w, h = map(int, head[1].split())
        p7 = (out / 'native.p7').read_bytes(); rgba = p7.split(b'ENDHDR\n', 1)[1]
        assert len(rgb) == w*h*3 and len(rgba) == w*h*4 and all(rgba[c::4] == rgb[c::3] for c in range(3))
        assert report['pid'] == pid and [report['width'], report['height']] == [w, h]
        pngb = png(w, h, rgb); (E / f'{mode}-{s}.png').open('xb').write(pngb)
        item.update(helper=report, pixels=[w, h], p6=str(out / 'native.p6'), p7=str(out / 'native.p7'), p6_sha256=sha(p6), p7_sha256=sha(p7), rgb_sha256=sha(rgb), rgba_sha256=sha(rgba), png=f'{mode}-{s}.png', png_sha256=sha(pngb), alpha_min=min(rgba[3::4]), alpha_max=max(rgba[3::4]))
    item['fixture_unchanged'] = sha(fixture.read_bytes()) == fixture_sha
    results.append(item); print(json.dumps({k: v for k, v in item.items() if k not in ('log', 'helper')})[:700], flush=True)
(E / f'native-{s}-{"-".join(sys.argv[1:])}.json').open('x').write(json.dumps(results, indent=2) + '\n')
