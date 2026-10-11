"""Launch only owned 640x400 hosts: actual compact input, WGPU readback and self-close."""
import datetime
import json
import os
from pathlib import Path
import subprocess
import sys
root = Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0110')
worktree = Path('/home/charlesmowbray/Development/viamind-t-0110')
source = sys.argv[1]
prefix = sys.argv[2] if len(sys.argv) > 2 else ''
binary = root / ('inspection-' + source)
for mode in ['row', 'completed']:
    name = prefix + mode
    command = [str(binary), mode, str(root / (name + '.ppm')), str(root / (name + '-definition.json'))]
    with (root / (name + '-native.log')).open('xb') as log:
        child = subprocess.Popen(command, cwd=worktree, stdout=log, stderr=subprocess.STDOUT,
                                 env=dict(os.environ, TMPDIR='/home/charlesmowbray/.cache/viamind/tmp'))
        record = {'command': command, 'pid': child.pid, 'source_commit': source,
                  'launched_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  'scope': 'Actual compact640x400 egui input preparation; actual native WGPU Screenshot, no OS input/compositor claim'}
        try:
            record['exit_code'] = child.wait(timeout=60)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=5)
            raise
        record['exited_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        with (root / (name + '-launch.json')).open('x') as file:
            json.dump(record, file, indent=2)
            file.write('\n')
        assert record['exit_code'] == 0, record
        assert (root / (name + '.ppm')).is_file()
        assert not Path('/proc', str(child.pid)).exists()
        print(name, child.pid, 'self-closed exit0', flush=True)
