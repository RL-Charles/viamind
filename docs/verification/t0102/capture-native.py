"""Run only owned immutable inspection hosts; each fsyncs readback then closes."""
import datetime
import json
import os
from pathlib import Path
import subprocess

root = Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0102')
worktree = Path('/home/charlesmowbray/Development/viamind-t-0102')
import sys
source_commit = sys.argv[1]
binary = root / ('inspection-' + source_commit)
source = worktree / 'fixtures/symbols/original-regulator.viamind-symbol.json'
for name, mode, width, height in [('wide-final', 'preview', 1600, 1100), ('compact-final', 'preview', 640, 400), ('completed-final', 'completed', 1600, 1100)]:
    name = (sys.argv[2] if len(sys.argv) > 2 else '') + name
    command = [str(binary), str(source), str(root / (name + '-new-definition.json')), mode, str(width), str(height), str(root / (name + '.ppm'))]
    with (root / (name + '-native.log')).open('xb') as log:
        process = subprocess.Popen(command, cwd=worktree, stdout=log, stderr=subprocess.STDOUT,
                                   env=dict(os.environ, TMPDIR='/home/charlesmowbray/.cache/viamind/tmp'))
        record = {'command': command, 'pid': process.pid, 'source_commit': source_commit,
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
