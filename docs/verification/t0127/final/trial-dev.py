import json, subprocess, sys, time
from pathlib import Path
E = Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0127'); n = sys.argv[1]
for mode in ['compact', 'wide']:
    w = E / f'launch-dev{n}-{mode}.sh'
    flag = ' --compact' if mode == 'compact' else ''
    w.write_text(f"#!/bin/sh\nexport TMPDIR=/home/charlesmowbray/.cache/viamind/tmp\nexec {E}/inspection-dev-{n} {E}/dev-fixture.viamind.json {E}/native-dev{n}-{mode}{flag} > {E}/launch-dev{n}-{mode}.sh.log 2>&1\n")
    w.chmod(0o755)
    r = subprocess.run(['cua-driver', 'launch_app', json.dumps({'launch_path': str(w)})], capture_output=True, text=True, timeout=120)
    pid = json.loads(r.stdout)['pid']; t = time.time()
    while Path(f'/proc/{pid}').exists() and time.time() - t < 60: time.sleep(0.25)
    log = (E / f'launch-dev{n}-{mode}.sh.log').read_text()
    out = E / f'native-dev{n}-{mode}'
    print(mode, 'exited', not Path(f'/proc/{pid}').exists(), 'files', sorted(p.name for p in out.iterdir()) if out.exists() else None)
    print('\n'.join(l[:400] for l in log.splitlines() if 'panicked' in l or l.startswith('exact')))
