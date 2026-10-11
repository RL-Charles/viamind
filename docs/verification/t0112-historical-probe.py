#!/usr/bin/env python3
"""Inspect only explicitly owned scratch releases using two actual read-only MCP clients."""
import sys
sys.dont_write_bytecode=True
import argparse, csv, hashlib, importlib.util, io, json, os, subprocess, time
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary',type=Path,required=True);p.add_argument('--previous-binary',type=Path,required=True);p.add_argument('--repo',type=Path,required=True);p.add_argument('--native',type=Path,required=True);p.add_argument('--directory',type=Path,required=True);p.add_argument('--endpoint',required=True)
a=p.parse_args();root=a.directory.resolve();root.mkdir();binary=a.binary.resolve(strict=True);previous=a.previous_binary.resolve(strict=True);native=a.native.resolve(strict=True)
sys.path.insert(0,str(a.repo/'scripts'))
from mcp_reference import digest,read_regular,StdioClient
spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
checks=[];report={'passed':False,'checks':checks,'binary':str(binary),'binary_sha256':digest(binary),'previous_binary_sha256':digest(previous),'endpoint':a.endpoint,'scope':'Detached fixed-layout length/assembly identity consistency only. No geometry/DRC recomputation, authenticated file content, source-board equivalence or manufacturer acceptance.'}
def check(name,ok):
    assert ok,name
    checks.append(name)
def run(args,tool=binary):
    env=os.environ.copy();env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
    return subprocess.run([str(tool),*map(str,args)],env=env,capture_output=True,timeout=90)
def inspect(path):
    r=run(['--inspect-output-release',path]);assert r.returncode==0,r.stderr.decode();return json.loads(r.stdout)
clients=[]
try:
    clients=[smoke.McpClient(a.endpoint,'Saved release reader A'),smoke.McpClient(a.endpoint,'Saved release reader B')]
    def call(name,args=None,error=False,peer=False):return clients[int(peer)].call(name,args or {},expect_error=error)
    names={t['name'] for t in clients[0].rpc('tools/list',{})['tools']}
    check('actual startup read-only discovery advertises inspector and hides release writer','inspect_output_release' in names and 'run_output_job' not in names)
    before=call('get_session_state');status=call('get_document_status');native_bytes=read_regular(native)
    source=root/'historical.viamind.json';project=json.loads(read_regular(a.repo/'fixtures/assembly/output-job-population.viamind.json'))
    # Native wrapper field layout is detected explicitly; no existing source is edited.
    document=project['project'] if 'project' in project else project
    document['id']='00000000-0000-0000-0000-000000000099'
    source.write_text(json.dumps(project))
    releases={};payloads={}
    for version in (1,2,3):
        for checked in (False,True):
            label=f'v{version}-'+('checked' if checked else 'generic')
            job={'format':'viamind-output-job','version':version,'name':'Original detached reader proof','profile':'checked_outer_2layer' if checked else 'generic_outer_2layer','mask_expansion_mm':0.0}
            if checked:job['checks']={'copper_clearance_mm':0.0,'edge_clearance_mm':0.0,'minimum_via_drill_mm':0.0,'minimum_annular_width_mm':0.0,'hole_clearance_mm':0.0,'hole_to_copper_clearance_mm':0.0,'max_violations':1000}
            if version>=2:job['include_calibrated_placement']=True
            if version==3:job['include_populated_bom']=True
            settings=root/(label+'.json');settings.write_text(json.dumps(job));release=root/label
            exported=run(['--run-output-job',settings,'--open',source,'--output',release]);check(label+' current production export',exported.returncode==0)
            manifest=json.loads(read_regular(release/'release-manifest.json'));expected_files=10+version+int(checked)
            current=inspect(release)
            check(label+' display-free CLI scalar/layout/length facts',current['version']==version and current['file_count']==expected_files and current['exported_cursor']==manifest['exported_cursor'] and current['observed_bytes']==sum(len(read_regular(release/name)) for name in [*manifest['files'],'release-manifest.json']))
            for peer in (False,True):check(label+' actual HTTP client '+str(int(peer))+' exact CLI parity',call('inspect_output_release',{'path':str(release)},peer=peer)==current)
            check(label+' historical provenance is separate from current document',current['project_id']!=before['project']['id'] and current['project_id']==manifest['project_id'])
            old=root/(label+'-prior');r=run(['--run-output-job',settings,'--open',source,'--output',old],previous);assert r.returncode==0,r.stderr.decode()
            check(label+' prior/current unchanged format and config bytes',all(read_regular(release/name)==read_regular(old/name) for name in manifest['files'] if name!='validation/drc.json'))
            releases[label]=release;payloads[label]={name:read_regular(release/name) for name in [*manifest['files'],'release-manifest.json']}
    release=releases['v3-generic'];manifest=json.loads(payloads['v3-generic']['release-manifest.json']);current=inspect(release)
    bom=list(csv.DictReader(io.StringIO(read_regular(release/'assembly/populated-bom.csv').decode(),newline='')));placement=list(csv.DictReader(io.StringIO(read_regular(release/'assembly/calibrated-placement.csv').decode(),newline='')))
    import uuid
    ids=sorted(uuid.UUID(row['Component ID']) for row in bom);expected_digest=hashlib.sha256(b'viamind-populated-components-v1\0'+b''.join(item.bytes for item in ids)).hexdigest()
    check('independent CSV UUID/hashlib reader verifies matched set/count/digest',set(row['Component ID'] for row in bom)==set(row['Component ID'] for row in placement) and current['matched_assembly']=={'component_count':len(ids),'component_ids_sha256':expected_digest} and expected_digest==manifest['populated_assembly']['component_ids_sha256'])
    check('independent reader preserves escaped Unicode quotes and multiline exact text',any('µ' in row['Value'] and '"' in row['Value'] and '\n' in row['Value'] for row in bom))
    for args in ({'path':str(release),'extra':True},{'path':'x'*4097},{'path':str(native)},{'path':str(root/'missing')}):check('strict inspector request/path refusal '+str(len(checks)),bool(call('inspect_output_release',args,error=True)))
    manifest_path=release/'release-manifest.json';original=read_regular(manifest_path)
    for mode in ('traversal','length','digest','null'):
        changed=json.loads(original)
        if mode=='traversal':changed['files']['../escape']=changed['files'].pop('job.json')
        elif mode=='length':changed['files']['assembly/bom.csv']+=1
        elif mode=='digest':changed['populated_assembly']['component_ids_sha256']='0'*64
        else:changed['calibrated_placement']=None
        try:manifest_path.write_text(json.dumps(changed));check('actual reader rejects '+mode+' contradiction',bool(call('inspect_output_release',{'path':str(release)},error=True)))
        finally:manifest_path.write_bytes(original)
    copper=release/'fabrication/top-copper.gbr';old=read_regular(copper)
    try:
        copper.write_bytes(b'!'+old[1:]);check('same-length unchecked content honestly passes declared checks only',call('inspect_output_release',{'path':str(release)})==current)
    finally:copper.write_bytes(old)
    bom_path=release/'assembly/populated-bom.csv';old=read_regular(bom_path)
    try:
        bom_path.write_bytes(old.replace(b'A_TOP',b'X_TOP',1));check('same-length shared component field mismatch rejects',bool(call('inspect_output_release',{'path':str(release)},error=True)))
    finally:bom_path.write_bytes(old)
    regular=release/'assembly/bom.csv';backup=release/'assembly/original.csv';regular.rename(backup)
    try:
        regular.symlink_to(backup);check('final symlink refuses',bool(call('inspect_output_release',{'path':str(release)},error=True)));regular.unlink();os.mkfifo(regular);start=time.monotonic();check('FIFO refuses without blocking',bool(call('inspect_output_release',{'path':str(release)},error=True)) and time.monotonic()-start<2)
    finally:
        if regular.exists() or regular.is_symlink():regular.unlink()
        backup.rename(regular)
    check('all original release payloads restored exactly',all(read_regular(releases[label]/name)==data for label,files in payloads.items() for name,data in files.items()))
    check('both observers preserve complete live session/status/native',call('get_session_state')==before and call('get_document_status')==status and read_regular(native)==native_bytes)
    with StdioClient([str(binary),'--headless','--agent-read-only','--open',str(native)],root,timeout=90) as stdio:
        stdio.initialize();names={tool['name'] for tool in stdio.discover()['tools']};check('actual read-only stdio advertises inspector', 'inspect_output_release' in names and 'run_output_job' not in names)
        check('actual stdio and HTTP agree on historical release',stdio.call('inspect_output_release',{'path':str(release)})==current);stdio.finish()
    check('retained current and prior executable identities unchanged',digest(binary)==report['binary_sha256'] and digest(previous)==report['previous_binary_sha256'])
    report.update(passed=True,check_count=len(checks),native_sha256=digest(native),historical_release=str(release),observed_report=current)
finally:
    for client in clients:client.close()
    with (root/'verification.json').open('x') as out:json.dump(report,out,indent=2);out.flush();os.fsync(out.fileno())
print(json.dumps({'passed':report['passed'],'checks':len(checks),'directory':str(root)}))
