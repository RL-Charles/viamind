"""Actual production two-agent HTTP / display-free CLI companion verification."""
import argparse,csv,hashlib,importlib.util,io,json,math,os,subprocess,sys
sys.dont_write_bytecode=True
from pathlib import Path
p=argparse.ArgumentParser()
for k in ['binary','previous-binary','project','repo','directory']:p.add_argument('--'+k,type=Path,required=True)
p.add_argument('--endpoint',required=True);a=p.parse_args();root=a.directory.resolve();root.mkdir(parents=True,exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
report={'passed':False,'checks':[],'endpoint':a.endpoint,'binary_sha256':sha(a.binary),'previous_binary_sha256':sha(a.previous_binary),'fixture_sha256':sha(a.project)}
def check(name,condition):
    assert condition,name
    report['checks'].append(name)
env=os.environ.copy()
for key in ['DISPLAY','WAYLAND_DISPLAY']:env.pop(key,None)
def cli(binary,job,path):return subprocess.run([str(binary.resolve()),'--run-output-job',str(job),'--open',str(a.project),'--output',str(path)],env=env,capture_output=True,text=True,timeout=60)
def files(path):return {str(p.relative_to(path)):p.read_bytes() for p in path.rglob('*') if p.is_file()}
def oracle(project,data):
    rows=list(csv.DictReader(io.StringIO(data.decode(),newline='')));byref={r['Designator']:r for r in rows}
    expected=[p for p in project['board']['parts'].values() if p.get('assembly',{}).get('populate',True)]
    assert len(rows)==len(byref)==len(expected)
    for part in expected:
        row=byref[part['designator']];cal=part['assembly']['placement'];r=math.radians(part['rotation_mdeg']/1000);theta=math.radians(cal['zero_axis_mdeg']/1000);sign=-1 if part['side']=='bottom' else 1
        cx=cal['centroid_local']['x']/1e6;cy=cal['centroid_local']['y']/1e6
        x=part['position']['x']/1e6+sign*cx*math.cos(r)+cy*math.sin(r)
        y=-(part['position']['y']/1e6-sign*cx*math.sin(r)+cy*math.cos(r))
        angle=math.degrees(math.atan2(sign*math.cos(theta)*math.sin(r)+math.sin(theta)*math.cos(r),sign*math.cos(theta)*math.cos(r)-math.sin(theta)*math.sin(r)))%360
        assert abs(float(row['X (mm, global)'])-x)<=1e-6 and abs(float(row['Y (mm, up)'])-y)<=1e-6
        assert abs(float(row['Rotation (deg CCW, top view)'])-angle)<=1e-6
        assert row['Side']==part['side'] and row['Mounting']==part['assembly']['mounting']
    return True
spec=importlib.util.spec_from_file_location('smoke',a.repo/'scripts/mcp-smoke.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
reader=m.McpClient(a.endpoint,'job-companion-reader');peer=m.McpClient(a.endpoint,'job-companion-peer')
try:
    before=reader.call('get_session_state',{});status=reader.call('get_document_status',{});active=Path(status['path']);native=active.read_bytes();report['initial_cursor']=before['cursor']
    check('two initialized HTTP clients share complete native session',reader.session!=peer.session and peer.call('get_session_state',{})==before)
    tools={t['name']:t for t in reader.rpc('tools/list')['tools']};schema=tools['save_output_job']['inputSchema']
    check('actual production schema exposes strict explicit companion version',all(x in json.dumps(schema) for x in ['include_calibrated_placement','const','oneOf']))
    old={'format':'viamind-output-job','version':1,'name':'Explicit companion verification','profile':'generic_outer_2layer','mask_expansion_mm':0.0}
    job=dict(old,version=2,include_calibrated_placement=True);settings=root/'companion.json'
    reader.call('save_output_job',{'path':str(settings),'job':job})
    check('second client loads exact canonical version2 settings',peer.call('load_output_job',{'path':str(settings)})['job']==job)
    req={'job_path':str(settings),'expected_cursor':before['cursor']}
    first=root/'http-a';second=root/'http-b'
    result=reader.call('run_output_job',dict(req,path=str(first)));peer.call('run_output_job',dict(req,path=str(second)))
    check('two clients publish exact twelve-file coherent releases',len(files(first))==12 and files(first)==files(second) and result['file_count']==12 and result['exported_cursor']==before['cursor'])
    companion=first/'assembly/calibrated-placement.csv';summary=result['manifest']['calibrated_placement']
    check('manifest declares populated companion and generic coordinate convention',result['manifest']['version']==2 and summary['component_count']==2 and summary['coordinate_convention']=='global_x_right_y_up_ccw_common_top_view')
    check('independent CSV/vector reader verifies transformed centroids and top-view markers',oracle(before['project'],companion.read_bytes()))
    standalone=root/'standalone.csv';reader.call('export_assembly_csv',{'kind':'assembly_placement','path':str(standalone),'expected_cursor':before['cursor']})
    check('job companion preserves exact standalone shared CSV bytes',standalone.read_bytes()==companion.read_bytes())
    out=root/'cli-release';command=cli(a.binary,settings,out)
    check('display-free actual production CLI publishes version2',command.returncode==0 and len(files(out))==12)
    check('CLI and HTTP format/config payloads are byte identical excluding session manifest',{k:v for k,v in files(out).items() if k!='release-manifest.json'}=={k:v for k,v in files(first).items() if k!='release-manifest.json'})
    oldpath=root/'default.json';reader.call('save_output_job',{'path':str(oldpath),'job':old})
    prior=root/'prior-default';current=root/'current-default';one=cli(a.previous_binary,oldpath,prior);two=cli(a.binary,oldpath,current)
    check('version1 old production ten config/format payload bytes remain identical',one.returncode==two.returncode==0 and len(files(prior))==len(files(current))==11 and {k:v for k,v in files(prior).items() if k!='release-manifest.json'}=={k:v for k,v in files(current).items() if k!='release-manifest.json'})
    default_manifest=json.loads((current/'release-manifest.json').read_bytes())
    check('version1 manifest omits optional companion unchanged',default_manifest['version']==1 and 'calibrated_placement' not in default_manifest)
    for i,bad in enumerate([dict(job,version=1),dict(job,include_calibrated_placement=False),dict(job,include_calibrated_placement=None),{k:v for k,v in job.items() if k!='include_calibrated_placement'},dict(job,outputs=['assembly_placement'])]):
        path=root/f'invalid-{i}.json';reader.call('save_output_job',{'path':str(path),'job':bad},expect_error=True);check('invalid job combination '+str(i)+' refuses NEW file',not path.exists())
    duplicate=root/'duplicate.json';duplicate.write_text(json.dumps(job)[:-1]+',"include_calibrated_placement":true}')
    peer.call('load_output_job',{'path':str(duplicate)},expect_error=True)
    check('duplicate explicit option is rejected',duplicate.exists())
    before_files=files(first);peer.call('run_output_job',dict(req,path=str(first)),expect_error=True)
    check('NEW collision leaves all release bytes unchanged',files(first)==before_files)
    peer.call('run_output_job',dict(req,path=str(active)),expect_error=True)
    check('MCP protects current active native path',active.read_bytes()==native)
    check('CLI protects its own opened native file and existing release',cli(a.binary,settings,a.project).returncode!=0 and cli(a.binary,settings,out).returncode!=0 and active.read_bytes()==native)
    checks={k:0.0 for k in ['copper_clearance_mm','edge_clearance_mm','minimum_via_drill_mm','minimum_annular_width_mm','hole_clearance_mm','hole_to_copper_clearance_mm']};checks['max_violations']=1000
    checked=dict(job,profile='checked_outer_2layer',checks=checks);checkedpath=root/'checked.json';reader.call('save_output_job',{'path':str(checkedpath),'job':checked})
    checkedout=root/'checked';value=peer.call('run_output_job',{'job_path':str(checkedpath),'path':str(checkedout),'expected_cursor':before['cursor']})
    drc=json.loads((checkedout/'validation/drc.json').read_bytes())
    check('checked companion uses production DRC on same capture and thirteen files',value['file_count']==13 and drc['report']['violations']==[] and (checkedout/'assembly/calibrated-placement.csv').read_bytes()==companion.read_bytes())
    check('all accepted/refused outputs preserve full session and document status',reader.call('get_session_state',{})==before and reader.call('get_document_status',{})==status)
    id=next(iter(before['project']['board']['parts']));original=before['project']['board']['parts'][id]
    peer.call('set_part_assembly',{'id':id,'assembly':{'mounting':'hybrid'},'expected':original,'expected_project':before['project']['id']})
    changed=reader.call('get_session_state',{});failed=root/'incomplete';peer.call('run_output_job',dict(req,path=str(failed),expected_cursor=changed['cursor']),expect_error=True)
    check('partial explicit calibration refuses entire release atomically',not failed.exists() and reader.call('get_session_state',{})==changed)
    stale=root/'stale';peer.call('run_output_job',dict(req,path=str(stale)),expect_error=True)
    check('complete stale cursor refuses before publication',not stale.exists())
    peer.call('undo',{})
    peer.call('set_part_assembly',{'id':id,'assembly':dict(original['assembly'],populate=False),'expected':original,'expected_project':before['project']['id']})
    population=reader.call('get_session_state',{});popout=root/'population';value=reader.call('run_output_job',dict(req,path=str(popout),expected_cursor=population['cursor']))
    check('population applies only to companion with explicit subset count',value['manifest']['calibrated_placement']['component_count']==1 and oracle(population['project'],(popout/'assembly/calibrated-placement.csv').read_bytes()) and (popout/'assembly/bom.csv').read_bytes()==(first/'assembly/bom.csv').read_bytes())
    peer.call('undo',{})
    current=reader.call('get_session_state',{});peer.call('set_named_net_rules',{'expected_project':before['project']['id'],'expected_cursor':current['cursor'],'rules':[{'net':'GND','minimum_copper_clearance':0,'minimum_track_width':250001}]})
    policy=reader.call('get_session_state',{});refused=root/'policy-refused';error=reader.call('run_output_job',{'job_path':str(checkedpath),'path':str(refused),'expected_cursor':policy['cursor']},expect_error=True)
    check('zero saved baselines cannot weaken captured document width minimum','track width' in error and not refused.exists() and reader.call('get_session_state',{})==policy)
    peer.call('undo',{});final=reader.call('get_session_state',{})
    check('peer edits and undo restore exact project/native status with advanced cursor',final['project']==before['project'] and final['cursor']['sequence']==before['cursor']['sequence']+6 and reader.call('get_document_status',{})==status and active.read_bytes()==native)
    report['final_cursor']=final['cursor'];report['passed']=True
finally:
    reader.close();peer.close();(root/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
