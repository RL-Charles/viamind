"""Run only owned immutable inspection hosts; each fsyncs readback then closes."""
import datetime
import json
import os
from pathlib import Path
import subprocess

root = Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0098')
worktree = Path('/home/charlesmowbray/Development/viamind-t-0098')
binary = root / 'inspection-83bc7d5'
source = worktree / 'fixtures/footprints/original-detached-asymmetric.viamind-footprint.json'
for name, mode, width, height in [('normal', 'preview', 1600, 1100), ('compact', 'preview', 640, 400), ('completed', 'completed', 1600, 1100)]:
    command = [str(binary), str(source), str(root / (name + '-new-definition.json')), mode, str(width), str(height), str(root / (name + '.ppm'))]
    with (root / (name + '-native.log')).open('xb') as log:
        process = subprocess.Popen(command, cwd=worktree, stdout=log, stderr=subprocess.STDOUT,
                                   env=dict(os.environ, TMPDIR='/home/charlesmowbray/.cache/viamind/tmp'))
        record = {'command': command, 'pid': process.pid, 'source_commit': '83bc7d515c01eb81b8ff58284d5ca96e9ae2f0b8',
                  'launched_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  'scope': 'Actual egui input preparing production ViamindApp; actual native WGPU Screenshot; no OS input or compositor capture claim'}
        try:
            record['exit_code'] = process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
            raise
        record['exited_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        with (root / (name + '-launch.json')).open('x') as file:
            json.dump(record, file, indent=2)
            file.write('\n')
        assert record['exit_code'] == 0, (name, record['exit_code'])
        assert (root / (name + '.ppm')).is_file()
        print(name, process.pid, 'self-closed exit0', flush=True)
