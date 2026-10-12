"""Copy final T121 receipts into the repository and write the evidence master."""
import hashlib, json, shutil, sys
from pathlib import Path
E = Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0121'); R = Path('/home/charlesmowbray/Development/viamind-t-0121')
D = R / 'docs/verification/t0121/final'; s = '02305397'; S = '023053973d8e1932a5339deafb49fbc66ff61e3d'
def sha(b): return hashlib.sha256(b).hexdigest()
D.mkdir(parents=True, exist_ok=False)
names = ['final1-attempt0-dirty-pycache.log', 'final1.sh', 'final1.log', 'final2.sh', 'final2.log', 'aperture-oracle1.sh', 'aperture-oracle1.log',
         'final3.sh', 'final3.log', 'final3-binaries.sha256', 'native-02305397.py', 'native-02305397-placed.json', 'native-02305397-compact-board-completed.json',
         'production-02305397-launch.json', 'production-02305397-cua-state.json', 'production-02305397-cua.png', 'http-02305397.log',
         'production-probe.py', 'checked-production-probe.py', 'inspection-fixture.viamind.json', 'assemble-final.py']
names += [f'{m}-{s}{x}' for m in ['placed', 'compact', 'board', 'completed'] for x in ['.png']]
names += [f'native-{m}-{s}-launch.json' for m in ['placed', 'compact', 'board', 'completed']]
names += [f'launch-{m}-{s}.sh' for m in ['placed', 'compact', 'board', 'completed', 'production']]
names += [f'{m}-{s}.footprint.json' for m in ['completed']] + [f'completed-{s}.footprint.source.json']
for n in names: shutil.copy2(E / n, D / n)
for d in [f'http-{s}', f'stdio-{s}', f'checked-stdio-{s}', f'workflow-{s}', f'catalog-{s}-report']: shutil.copytree(E / d, D / d)
artifacts = []
for p in sorted(x for x in D.rglob('*') if x.is_file()):
    b = p.read_bytes(); artifacts.append({'storage': 'repository', 'path': str(p.relative_to(R)), 'bytes': len(b), 'sha256': sha(b)})
retained = [f'viamind-{s}', f'inspection-{s}'] + [f'{m}-{s}.{x}' for m in ['placed', 'compact', 'board', 'completed'] for x in ['ppm', 'p7']]
for n in retained:
    b = (E / n).read_bytes(); artifacts.append({'storage': 'retained', 'path': str(E / n), 'bytes': len(b), 'sha256': sha(b)})
native = []
for f in ['native-02305397-placed.json', 'native-02305397-compact-board-completed.json']:
    for item in json.loads((E / f).read_text()):
        native.append({k: item[k] for k in ['mode', 'pid', 'exited', 'seconds', 'pixels', 'p6_sha256', 'p7_sha256', 'rgb_sha256', 'rgba_sha256', 'png_sha256', 'alpha_min', 'alpha_max']}
                      | {'png': f"{item['mode']}-{s}.png", 'p6': str(E / f"{item['mode']}-{s}.ppm"), 'p7': str(E / f"{item['mode']}-{s}.p7"), 'helper': item['helper']})
def receipt(p): d = json.loads((D / p).read_text()); return {'passed': d['passed'], 'checks': len(d['checks']), 'binary_sha256': d['binary_sha256'], 'endpoint': d.get('endpoint')}
binaries = dict(reversed(l.split()) for l in (E / 'final3-binaries.sha256').read_text().splitlines())
master = {
    'ticket': 'T-0121', 'source': S, 'base_main': '88c3ec12db48e7fff0aae84ca0d96bbf65ab668c',
    'pre_transplant': {'tip': '3033041827cf1aaa3f8c545e1cde3e8bd4001524', 'tag': 'verification/t0121/pre-transplant-30330418', 'own_range': '8f5af1d653cade9d3e9e019e80a601964a98611d..3033041827cf1aaa3f8c545e1cde3e8bd4001524',
                       'range_diff': '25 own commits preserved: 23 identical, 2 differ only in merge context (additive UiAction and adjacent test module)'},
    'integration_commits': {'0b3b377f': 'holes join connected-drag identity set and schematic_budget aggregate; drag Part fixtures holes:None',
                            '9f900007': 'hole editor test edits routed through UiAction catalog (command_parity gate)',
                            '02305397': 'aperture compact test matches pinned control by own painted frame'},
    'full_linux': {'source': S, 'log': 'docs/verification/t0121/final/final3.log', 'exit': 0, 'fmt': 'pass', 'strict_workspace_all_target_clippy': 'pass',
                   'workspace_tests': {'passed': 1072, 'failed': 0, 'ignored': 1, 'suites': 244}, 'workspace_build': 'pass', 'retouched_before_normal_build': True,
                   'catalog_tools': 121, 'workflow_checks': 18, 'python_tests': 13},
    'failed_attempts': {'final1-attempt0': 'refused before cargo: stray scripts/__pycache__ made worktree dirty',
                        'final1': 'fmt+clippy pass; command_parity refused direct engine calls in hole editor test (EXIT 101)',
                        'final2': 'fmt+clippy pass; 1071 passed/1 failed: aperture compact clip heuristic matched scrolled hole row (EXIT 101)'},
    'binaries': {'application': {'path': str(E / f'viamind-{s}'), 'sha256': binaries[str(E / f'viamind-{s}')]}, 'inspection': {'path': str(E / f'inspection-{s}'), 'sha256': binaries[str(E / f'inspection-{s}')]}},
    'production': {'pid': json.loads(json.loads((D / 'production-02305397-launch.json').read_text())['stdout'])['pid'], 'port': 7963, 'launch': 'cua-driver launch_app owned wrapper'},
    'actual_checks': {'stdio': receipt(f'stdio-{s}/production.json'), 'two_http_clients': receipt(f'http-{s}/production.json'), 'checked_callback_stdio': receipt(f'checked-stdio-{s}/checked-production.json')},
    'native': {'captures': native, 'method': 'actual egui Harness input prepares production UI; state/memory transferred to eframe; native WGPU Screenshot event; no OS input or cadence claim'},
    'limitations': ['No manufacturing/DFM certification or minimum-drill rule inferred.', 'Native rendering is not boolean copper clipping.', 'Only circular NPTH; no slots/oval/plated mechanical holes.'],
    'artifacts': artifacts}
(R / 'docs/verification/t0121/master.json').open('x').write(json.dumps(master, indent=2) + '\n')
print(len(artifacts), 'artifacts')
