"""Assemble docs/verification/t0127/master.json from retained T127 evidence (run once, NEW output)."""
import hashlib, json, sys
from pathlib import Path
sys.dont_write_bytecode = True
root = Path(__file__).resolve().parents[4]
base = root / 'docs/verification/t0127'
E = Path('/home/charlesmowbray/.cache/viamind/native-artifacts/t0127')
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def art(path, storage):
    p = root / path if storage == 'repository' else Path(path)
    return {'path': str(path), 'storage': storage, 'bytes': p.stat().st_size, 'sha256': sha(p)}
app = E / 'viamind-fb05911c'; helper = E / 'inspection-fb05911c'
captures = json.loads((E / 'native-fb05911c-wide-compact.json').read_text())
for c in captures:
    c.pop('log')
http = json.loads((base / 'final/http-4b116cbf/verification.json').read_text())
stdio = json.loads((base / 'final/stdio-fb05911c/verification.json').read_text())
master = {
    'ticket': 'T-0127',
    'source': 'fb05911c52edb4f241b779ef9a4a85f90a407158',
    'base_main': 'c92d3ae83df838f29bcb2e424cdb9fba7cda0505',
    'pre_transplant': {
        'tip': '56b75f93d1426af6ccd48508e68b7ff00e00f87b',
        'tag': 'verification/t0127/pre-transplant-56b75f93',
        'own_range': '4551f119..56b75f93 (12 commits; base 4551f119 = c404 NPTH foundation + T125, not copied)',
        'range_diff': "12 own commits preserved: 9 '=', 3 '!' only in merge context (schematic_budget holes line, additive UiActions, builtin_specs list)",
    },
    'integration_commits': {
        '9081516e': 'bus/entry UUIDs in doc validate_physical_identities and engine part_holes::identities; duplicate drag hole extend/test fields removed; collision and budget tests',
        '3cf32b7e': 'bus-free sheet SVG/PNG byte-identical to pre-bus renderer (oracle from main binary); CLI help lists buses',
        '98c77480': 'MCP reference catalog regenerated (127 tools); docs/schematic-buses-verification.md',
        '4b116cbf': 'inherited log15/log-base/1000-sheet tests aligned with feature 16 admission (final1 failures)',
        'fb05911c': 'inspection helper: ComboBox value query and native clock continuation (first native launches failed)',
    },
    'full_linux': {
        'source': 'fb05911c52edb4f241b779ef9a4a85f90a407158', 'log': 'docs/verification/t0127/final/final3.log', 'script': 'docs/verification/t0127/final/final-gates.sh',
        'exit': 0, 'fmt': 'pass', 'strict_workspace_all_target_clippy': 'pass',
        'workspace_tests': {'passed': 1104, 'failed': 0, 'ignored': 1, 'suites': 246},
        'workspace_build': 'pass', 'retouched_before_normal_build': True,
        'catalog_check': 'docs/generated exact, 127 tools', 'workflow_checks': 18, 'python_tests': 13, 'stdio_checks': len(stdio['checks']),
    },
    'failed_attempts': {
        'final1 (98c77480)': 'fmt+clippy pass; 3 inherited tests failed (log15 version text, validated log writer, 1000-sheet message); corrected in 4b116cbf',
        'final2 (4b116cbf)': 'fmt+clippy+1104 tests+build pass; script stopped at clean-tree assertion (untracked verifier written mid-run); post-gates-4b116cbf ran catalog/workflow/stdio PASS',
        'native 4b116cbf': 'both helper launches panicked before capture (ComboBox label query); dev builds showed transparent window (Harness-timed fade-in); fixed in fb05911c',
    },
    'binaries': {
        'application': {'path': str(app), 'sha256': sha(app)},
        'inspection': {'path': str(helper), 'sha256': sha(helper)},
    },
    'binary_identity': 'viamind-4b116cbf and viamind-fb05911c are byte-identical (058801...30e7); fb05911c changed only the inspection example',
    'production': {'pid': 297774, 'port': 7964, 'executable': str(E / 'viamind-4b116cbf'), 'executable_sha256': sha(E / 'viamind-4b116cbf'),
                   'launch': 'cua-driver launch_app owned wrapper launch-production-4b116cbf.sh', 'left_running': True},
    'actual_checks': {
        'stdio_three_modes': {'passed': stdio['passed'], 'checks': len(stdio['checks']), 'binary_sha256': stdio['binary_sha256'], 'endpoint': None},
        'stdio_plus_two_http_clients': {'passed': http['passed'], 'checks': len(http['checks']), 'binary_sha256': http['binary_sha256'], 'endpoint': 'http://127.0.0.1:7964/mcp'},
    },
    'native': {'captures': captures, 'route': 'production ViamindApp prepared with local egui Harness inputs; actual eframe WGPU Screenshot event; no OS input or cadence claim'},
    'render_oracle': {
        'svg': 'fixtures/views/legacy-oracle/schematic-default-c92d3ae8.svg', 'svg_sha256': sha(root / 'fixtures/views/legacy-oracle/schematic-default-c92d3ae8.svg'),
        'exported_by': '/home/charlesmowbray/.cache/viamind/native-artifacts/t0121/viamind-a530c641 (sha256 b7ff6506727bc300ce8afb8998f492a8d3d6fb4e3070a1d416f49e2ed1a011dc; crates/apps identical to c92d3ae8)',
        'test': 'crates/render/tests/schematic_buses.rs bus_free_sheet_default_output_is_byte_identical_to_pre_bus_renderer',
    },
    'limitations': ['no implicit bus conductivity, aliases, hierarchy, bus ERC/DRC or PCB continuity', 'native captures are egui Harness-prepared, not OS input; no frame cadence',
                    'HTTP receipt ran against the byte-identical 4b116cbf application copy', 'CI and independent review pending at assembly time'],
}
arts = [art(p.relative_to(root), 'repository') for p in sorted(base.rglob('*')) if p.is_file() and p.name not in ('master.json', 'verify-artifacts.py')]
arts.append(art('fixtures/views/legacy-oracle/schematic-default-c92d3ae8.svg', 'repository'))
retained = [app, helper, E / 'viamind-4b116cbf', E / 'production-fixture-4b116cbf.viamind.json', E / 'inspection-fixture-fb05911c.viamind.json',
            E / 'launch-wide-4b116cbf.sh.log', E / 'launch-compact-4b116cbf.sh.log', E / 'native-wide-fb05911c/native-report.json', E / 'native-compact-fb05911c/native-report.json',
            E / 'main-oracle/views-default.svg', E / 'main-oracle/views-default.png']
for c in captures:
    retained += [Path(c['p6']), Path(c['p7'])]
arts += [art(p, 'retained') for p in retained]
master['artifacts'] = arts
with (base / 'master.json').open('x') as f:
    json.dump(master, f, indent=2); f.write('\n')
print(len(arts), 'artifacts indexed')
