"""Actual production CLI/two HTTP clients, explicit generic marker convention."""
import argparse, csv, hashlib, importlib.util, io, json, math, os, subprocess, sys
sys.dont_write_bytecode = True
from pathlib import Path
p = argparse.ArgumentParser()
for key in ['binary','previous-binary','project','directory','repo']:
    p.add_argument('--'+key,type=Path,required=True)
p.add_argument('--endpoint',required=True)
a=p.parse_args(); root=a.directory.resolve(); root.mkdir(parents=True,exist_ok=False)
report={'passed':False,'checks':[],'endpoint':a.endpoint,
        'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest(),
        'previous_binary_sha256':hashlib.sha256(a.previous_binary.read_bytes()).hexdigest(),
        'fixture_sha256':hashlib.sha256(a.project.read_bytes()).hexdigest()}
def check(name, result):
    assert result, name
    report['checks'].append(name)
env=os.environ.copy()
for key in ['DISPLAY','WAYLAND_DISPLAY']: env.pop(key,None)
def run(binary,args):
    return subprocess.run([str(binary.resolve()),*map(str,args)],input='',env=env,capture_output=True,text=True,timeout=60)
def cli(kind,path,project=None,binary=None,overwrite=False):
    return run(binary or a.binary,['--export-csv',kind,'--open',project or a.project,'--output',path,*(['--overwrite'] if overwrite else [])])
def read_rows(data): return list(csv.DictReader(io.StringIO(data.decode(),newline='')))
def oracle(project, rows):
    byref={row['Designator']:row for row in rows}
    expected=[part for part in project['board']['parts'].values() if part.get('assembly',{}).get('populate',True)]
    assert len(byref)==len(expected)==len(rows)
    for part in expected:
        row=byref[part['designator']]; calibration=part['assembly']['placement']; r=math.radians(part['rotation_mdeg']/1000)
        theta=math.radians(calibration['zero_axis_mdeg']/1000); sign=-1 if part['side']=='bottom' else 1
        cx=calibration['centroid_local']['x']/1e6;cy=calibration['centroid_local']['y']/1e6
        x=part['position']['x']/1e6+sign*cx*math.cos(r)+cy*math.sin(r)
        y=-(part['position']['y']/1e6-sign*cx*math.sin(r)+cy*math.cos(r))
        ux=sign*math.cos(theta)*math.cos(r)-math.sin(theta)*math.sin(r)
        uy=sign*math.cos(theta)*math.sin(r)+math.sin(theta)*math.cos(r)
        angle=math.degrees(math.atan2(uy,ux))%360
        assert abs(float(row['X (mm, global)'])-x)<=1e-6
        assert abs(float(row['Y (mm, up)'])-y)<=1e-6
        assert abs(float(row['Rotation (deg CCW, top view)'])-angle)<=1e-6
        assert row['Side']==part['side'] and row['Mounting']==part['assembly']['mounting']
        assert row['Value']==part['value'] and row['Footprint']==part['footprint'] and row['Component ID']==part['id']
    return True
sys.path.insert(0,str(a.repo/'scripts'))
from mcp_reference import StdioClient
def canonical_save(binary,source,destination):
    command=['/usr/bin/env','-u','DISPLAY','-u','WAYLAND_DISPLAY',str(binary.resolve()),'--headless','--open',str(source),'--save-as',str(destination)]
    with StdioClient(command,root) as client:
        client.initialize()
        state=client.call('get_session_state',{})
        client.finish()
    return state['project']
spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
reader=m.McpClient(a.endpoint,'calibrated-placement-reader');peer=m.McpClient(a.endpoint,'calibrated-placement-peer')
try:
    before=reader.call('get_session_state',{});status=reader.call('get_document_status',{});active=Path(status['path']);native_bytes=active.read_bytes()
    report['initial_cursor']=before['cursor']
    check('two initialized native HTTP clients observe identical full session',reader.session!=peer.session and peer.call('get_session_state',{})==before)
    tools={t['name']:t for t in reader.rpc('tools/list')['tools']}
    check('actual export catalogue admits explicit calibrated placement','assembly_placement' in tools['export_assembly_csv']['inputSchema']['properties']['kind']['enum'])
    check('shared setter schema describes strict optional mounting and calibration',all(key in json.dumps(tools['set_part_assembly']['inputSchema']) for key in ['mounting','centroid_local','zero_axis_mdeg','through_hole']))
    parts=reader.call('query_parts',{})['parts'];check('borrowed query returns explicit metadata unchanged',all(part['assembly']==before['project']['board']['parts'][part['id']]['assembly'] for part in parts))
    cli_path=root/'cli-calibrated.csv';result=cli('assembly_placement',cli_path)
    check('no-display production calibrated CLI succeeds',result.returncode==0)
    expected=cli_path.read_bytes();rows=read_rows(expected)
    check('independent CSV/vector reader verifies non-origin top and reflected bottom centroids/markers',oracle(before['project'],rows))
    check('independent parser preserves Unicode comma and quotes',all(row['Value']=='µ, "explicit part"' for row in rows))
    path=root/'http-calibrated.csv';request={'kind':'assembly_placement','path':str(path),'expected_cursor':before['cursor']}
    value=reader.call('export_assembly_csv',request)
    check('native HTTP output is exact CLI bytes with included count and complete cursor',path.read_bytes()==expected and value['component_count']==2 and value['exported_cursor']==before['cursor'])
    other=root/'peer-calibrated.csv';peer.call('export_assembly_csv',dict(request,path=str(other)))
    check('two agent exports are byte identical',other.read_bytes()==expected)
    peer.call('export_assembly_csv',request,expect_error=True)
    check('NEW collision preserves complete output',path.read_bytes()==expected)
    peer.call('export_assembly_csv',dict(request,overwrite=True))
    check('explicit overwrite retains expected bytes',path.read_bytes()==expected)
    peer.call('export_assembly_csv',dict(request,path=str(active),overwrite=True),expect_error=True)
    check('native source is protected with explicit overwrite',active.read_bytes()==native_bytes)
    check('CLI collision and native overwrite are refused',cli('assembly_placement',cli_path).returncode!=0 and cli('assembly_placement',a.project,overwrite=True).returncode!=0 and active.read_bytes()==native_bytes)
    canonical=root/'canonical-nine.json';saved=canonical_save(a.binary,a.project,canonical)
    check('actual no-display initialized stdio/save/open preserves schema9 calibration',json.loads(canonical.read_bytes())['version']==9 and json.loads(canonical.read_bytes())['project']==saved==before['project'])
    legacy=json.loads(a.project.read_bytes());legacy['version']=7
    for part in legacy['project']['board']['parts'].values():
        part['assembly'].pop('mounting');part['assembly'].pop('placement')
    legacy_path=root/'legacy-sourcing.json';legacy_path.write_text(json.dumps(legacy,ensure_ascii=False,indent=2)+'\n')
    for kind in ['bom','placement','sourcing_bom','catalog_bom','assembly_bom']:
        current=root/(kind+'-current.csv');old=root/(kind+'-prior.csv')
        one,two=cli(kind,current),cli(kind,old,legacy_path,a.previous_binary)
        check('existing '+kind+' bytes unchanged with unused calibration present',one.returncode==two.returncode==0 and current.read_bytes()==old.read_bytes())
    legacy_new,legacy_old=root/'legacy-current-canonical.json',root/'legacy-prior-canonical.json'
    one=canonical_save(a.binary,legacy_path,legacy_new);two=canonical_save(a.previous_binary,legacy_path,legacy_old)
    check('absent new fields retain exact canonical schema7 native bytes',one==two==legacy['project'] and legacy_new.read_bytes()==legacy_old.read_bytes())
    settings=root/'generic-job.json'
    job={'format':'viamind-output-job','version':1,'name':'Calibration excludes job expansion','profile':'generic_outer_2layer','mask_expansion_mm':0.0}
    reader.call('save_output_job',{'path':str(settings),'job':job})
    current_release,prior_release=root/'current-release',root/'prior-release'
    one=run(a.binary,['--run-output-job',settings,'--open',a.project,'--output',current_release])
    two=run(a.previous_binary,['--run-output-job',settings,'--open',legacy_path,'--output',prior_release])
    current_files={str(p.relative_to(current_release)):p.read_bytes() for p in current_release.rglob('*') if p.is_file()}
    prior_files={str(p.relative_to(prior_release)):p.read_bytes() for p in prior_release.rglob('*') if p.is_file()}
    check('existing saved job ten format/config payload bytes ignore calibration unchanged',one.returncode==two.returncode==0 and len(current_files)==len(prior_files)==11 and {k:v for k,v in current_files.items() if k!='release-manifest.json'}=={k:v for k,v in prior_files.items() if k!='release-manifest.json'})
    unsupported=root/'unsupported-job.json'
    reader.call('save_output_job',{'path':str(unsupported),'job':dict(job,outputs=['assembly_placement'])},expect_error=True)
    check('fixed saved-job profile rejects unrequested calibrated output expansion',not unsupported.exists())
    check('all accepted exports and refusals preserve complete native session and status',reader.call('get_session_state',{})==before and reader.call('get_document_status',{})==status)
    id=next(part['id'] for part in parts if part['side']=='bottom');original=before['project']['board']['parts'][id]
    for invalid in [{'mounting':'inferred'},{'placement':{}},{'placement':{'centroid_local':{'x':0,'y':0,'z':0},'zero_axis_mdeg':0}},{'placement':{'centroid_local':{'x':0,'y':0},'zero_axis_mdeg':360000}}]:
        peer.call('set_part_assembly',{'id':id,'assembly':invalid,'expected':original},expect_error=True)
    check('invalid/partial/unknown calibration inputs are atomic',reader.call('get_session_state',{})==before)
    refused=root/'incomplete.csv'
    peer.call('set_part_assembly',{'id':id,'assembly':{'mounting':'hybrid'},'expected':original})
    error=reader.call('export_assembly_csv',dict(request,path=str(refused),expected_cursor=reader.call('get_session_state',{})['cursor']),expect_error=True)
    check('persistable partial metadata refuses whole calibrated export without file','calibration' in error and not refused.exists())
    peer.call('undo',{})
    metadata=dict(original['assembly'],populate=False)
    peer.call('set_part_assembly',{'id':id,'assembly':metadata,'expected':original})
    population=reader.call('get_session_state',{});pop_path=root/'population.csv'
    value=reader.call('export_assembly_csv',dict(request,path=str(pop_path),expected_cursor=population['cursor']))
    check('population excludes bottom row and reports included count',value['component_count']==1 and oracle(population['project'],read_rows(pop_path.read_bytes())))
    peer.call('undo',{})
    peer.call('update_part',{'id':id,'rotation_deg':33.7})
    changed=reader.call('get_session_state',{});arbitrary=root/'arbitrary.csv'
    reader.call('export_assembly_csv',dict(request,path=str(arbitrary),expected_cursor=changed['cursor']))
    check('non-cardinal native placement matches independent reflected-vector oracle',oracle(changed['project'],read_rows(arbitrary.read_bytes())))
    peer.call('set_part_assembly',{'id':id,'assembly':None,'expected':original},expect_error=True)
    check('second client complete Part guard rejects stale calibration clear',reader.call('get_session_state',{})==changed)
    reader.call('export_assembly_csv',dict(request,path=str(refused)),expect_error=True)
    check('complete original cursor stays stale after intervening undo/redo-history changes',not refused.exists())
    peer.call('undo',{})
    final=reader.call('get_session_state',{})
    check('three peer edits and undo restore exact project/native status with honest advanced cursor',final['project']==before['project'] and final['cursor']['sequence']==before['cursor']['sequence']+6 and reader.call('get_document_status',{})==status and active.read_bytes()==native_bytes)
    report['final_cursor']=final['cursor'];report['csv_sha256']=hashlib.sha256(expected).hexdigest();report['passed']=True
finally:
    reader.close();peer.close();(root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
