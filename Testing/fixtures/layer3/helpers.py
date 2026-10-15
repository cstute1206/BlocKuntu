"""Non-destructive helper identity and recovery-component availability checks."""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import pwd
import shutil
import subprocess
import time

from guest import WORK, rpc, run, wait
from lifecycle import finish, process_identity
from session_inventory import collect_processes


def supervisor(run_id):
    assert os.geteuid()==0
    account=pwd.getpwnam('akhi')
    def user():
        os.initgroups(account.pw_name,account.pw_gid)
        os.setgid(account.pw_gid)
        os.setuid(account.pw_uid)
    def user_rpc(method,params):
        code='import json,os,sys; from guest import rpc; assert os.geteuid()!=0; print(json.dumps(rpc(sys.argv[1],json.loads(sys.argv[2]))))'
        answer=subprocess.run(['/usr/bin/python3','-c',code,method,json.dumps(params)],cwd=WORK,preexec_fn=user,text=True,capture_output=True,timeout=15)
        assert answer.returncode==0,answer.stderr[-2000:]
        return json.loads(answer.stdout)
    def state(pid):
        try:
            stat=(Path('/proc')/str(pid)/'stat').read_text().rsplit(')',1)[1].split()
            return stat[0]
        except FileNotFoundError:
            return None
    processes=collect_processes()
    protected={pid for pid,p in processes.items() if Path(p['exe']).name in ('blockuntu-gui','blockuntu-native','blockuntud','blockuntu-focusd')}
    while True:
        expanded=protected|{pid for pid,p in processes.items() if p['ppid'] in protected}
        if expanded==protected:break
        protected=expanded
    gui=[pid for pid in protected if Path(processes[pid]['exe']).name=='blockuntu-gui']
    assert len(gui)==1,'Expected the running installed GUI'
    paths={p['exe'] for pid,p in processes.items() if p['uid']==account.pw_uid and pid not in protected}
    paths-={processes[pid]['exe'] for pid in protected}
    shared=Path('/opt/blockuntu-acceptance-helper')
    shared.mkdir(mode=0o755,exist_ok=False)
    for name in ('bk-test-parent','bk-test-app-b'):
        shutil.copyfile(WORK/name,shared/name)
        (shared/name).chmod(0o555)
    parent_path=str(shared/'bk-test-parent')
    helper_path=str(shared/'bk-test-app-b')
    paths.add(parent_path)
    paths.add(str(Path('/usr/bin/python3').resolve()))
    paths.update(str(Path(p).resolve()) for p in ('/usr/bin/cat','/usr/bin/dash') if Path(p).exists())
    paths.discard(helper_path)
    paths.discard('/usr/bin/blockuntu-native')
    # A second ordinary account must receive the same global process policy.
    try:second=pwd.getpwnam('bk-e2e-other')
    except KeyError:
        second_uid=next(uid for uid in range(1001,1100) if not any(p.pw_uid==uid for p in pwd.getpwall()))
        run('useradd','--create-home','--uid',str(second_uid),'--shell','/bin/bash','bk-e2e-other')
        second=pwd.getpwnam('bk-e2e-other')
    assert 1000<=second.pw_uid<65534 and second.pw_uid!=account.pw_uid
    def other_user():
        os.initgroups(second.pw_name,second.pw_gid)
        os.setgid(second.pw_gid)
        os.setuid(second.pw_uid)
    parent=native=recovered=other_parent=other_helper=other_recovered=None
    evidence={}
    try:
        ready=WORK/'helper-parent-ready.json'
        parent=subprocess.Popen([parent_path,'--lifetime','1800','--ready-file',str(ready),'--spawn-helper',helper_path],preexec_fn=user,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        wait(ready.exists,5)
        child=json.loads(ready.read_text())['child_pid']
        wait(lambda:(Path(f'/proc/{child}/comm').read_text().strip()=='bk-test-app-b'),5)
        child_stat=Path(f'/proc/{child}/stat').read_text().rsplit(')',1)[1].split()
        assert int(child_stat[1])==parent.pid
        helper={'pid':child,'exe':str(Path(f'/proc/{child}/exe').readlink()),'start_ticks':child_stat[19],'parent_pid':parent.pid}
        native=subprocess.Popen(['/usr/bin/blockuntu-native'],preexec_fn=user,stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        wait(lambda:Path(f'/proc/{native.pid}/exe').readlink().name=='blockuntu-native',5)
        native_identity=process_identity(native)
        other_parent=subprocess.Popen([parent_path,'--lifetime','1800'],preexec_fn=other_user,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        other_helper=subprocess.Popen([helper_path,'--lifetime','1800'],preexec_fn=other_user,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        wait(lambda:Path(f'/proc/{other_parent.pid}/comm').read_text().strip()=='bk-test-parent' and
             Path(f'/proc/{other_helper.pid}/comm').read_text().strip()=='bk-test-app-b',5)
        other_parent_identity={**process_identity(other_parent),'uid':collect_processes()[other_parent.pid]['uid']}
        other_helper_identity={**process_identity(other_helper),'uid':collect_processes()[other_helper.pid]['uid']}
        assert other_parent_identity['uid']==other_helper_identity['uid']==second.pw_uid
        rule_id=run_id+'-helper-allow'
        user_rpc('upsert_app_rule',{'rule':{'id':rule_id,'name':'Acceptance independent helper identities','tier':'scheduled_block','mode':'allowlist',
            'matchers':[{'kind':'executable_path','value':p} for p in sorted(paths)]}})
        session=user_rpc('start_detox',{'name':'Acceptance helper boundary','duration_minutes':2,'app_rule_ids':[rule_id]})['session']
        started=time.monotonic()
        wait(lambda:state(child) in (None,'Z') and other_helper.poll() is not None,30)
        termination=time.monotonic()-started
        assert parent.poll() is None and native.poll() is None and other_parent.poll() is None
        evidence={'rule_id':rule_id,'session':session,'parent_identity':process_identity(parent),'helper_identity':helper,
            'native_identity':native_identity,'gui_identity':{'pid':gui[0],'exe':processes[gui[0]]['exe'],'start_ticks':processes[gui[0]]['start_ticks']},
            'safety_paths':sorted(paths),'helper_terminated':True,'termination_seconds':termination,
            'protected_paths_omitted':not any(processes[p]['exe'] in paths for p in protected if processes[p]['uid']==account.pw_uid),
            'desktop_user_uid':account.pw_uid,'second_account_uid':second.pw_uid,
            'second_user_parent_identity':other_parent_identity,'second_user_helper_identity':other_helper_identity,
            'second_user_enforced':True,'second_user_termination_seconds':termination}
        assert evidence['protected_paths_omitted'],'Product descendants were explicitly allowed'
        (WORK/'helper-active.json').write_text(json.dumps(evidence))
        end=dt.datetime.fromisoformat(session['ends_at'].replace('Z','+00:00'))
        while dt.datetime.now(dt.timezone.utc)<end:
            assert parent.poll() is None and native.poll() is None and other_parent.poll() is None
            for pid in protected:
                if processes[pid]['uid']==account.pw_uid and (
                        pid in gui or Path(processes[pid]['exe']).name.startswith('WebKit')):
                    current=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()
                    assert current[0]!='Z' and current[19]==processes[pid]['start_ticks'],'Protected product process died'
            time.sleep(1)
        recovered=subprocess.Popen([helper_path,'--lifetime','600'],preexec_fn=user,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        wait(lambda:Path(f'/proc/{recovered.pid}/comm').read_text().strip()=='bk-test-app-b',5)
        evidence['restored_helper_identity']=process_identity(recovered)
        other_recovered=subprocess.Popen([helper_path,'--lifetime','600'],preexec_fn=other_user,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        wait(lambda:Path(f'/proc/{other_recovered.pid}/comm').read_text().strip()=='bk-test-app-b',5)
        evidence['second_user_restored_helper_identity']={**process_identity(other_recovered),
            'uid':collect_processes()[other_recovered.pid]['uid']}
        until=time.monotonic()+30
        while time.monotonic()<until:
            assert recovered.poll() is None and parent.poll() is None and native.poll() is None and other_parent.poll() is None and other_recovered.poll() is None
            time.sleep(1)
        evidence.update(parent_survived=True,native_survived=True,gui_survived=True,restored_helper_survived=True)
        evidence.update(second_user_control_survived=True,second_user_restored_helper_survived=True)
        return evidence
    finally:
        if native and native.stdin:native.stdin.close()
        for p in (parent,native,recovered,other_parent,other_helper,other_recovered):finish(p)


def run_cases(run_id,case):
    assert os.geteuid()!=0
    run('sudo','-n','systemd-run','--collect','--unit=blockuntu-helper-acceptance','--property=RuntimeMaxSec=240',
        '/usr/bin/python3',str(WORK/'helpers.py'),'--run-id',run_id)
    active=WORK/'helper-active.json'
    wait(lambda:active.exists() or (WORK/'helper-result.json').exists(),40)
    assert active.exists(),'Helper supervisor failed before active witness'
    from accessibility import click,texts
    from ui_controls import open_settings
    open_settings('Health')
    wait(lambda:'/run/blockuntu/blockuntud.sock mode 660' in texts(),30)
    assert rpc('status')['enforcement_state']=='active'
    control={'gui_health_available_during_restriction':True,'rpc_available_during_restriction':True}
    from guest import host_action
    host_action('screenshot',name='gui-helper-recovery-health')
    click('Close Settings')
    wait((WORK/'helper-result.json').exists,210)
    result=json.loads((WORK/'helper-result.json').read_text())
    assert result['status']=='pass',result.get('error')
    def checked():return {**result['evidence'],**control}
    for cid in ('VM-AAL-005','VM-AAL-019'):
        assert case(cid,checked),'Helper/recovery witness failed'


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--run-id',required=True)
    args=parser.parse_args();started=time.monotonic()
    try:row={'status':'pass','evidence':supervisor(args.run_id)}
    except Exception as error:row={'status':'fail','error':f'{type(error).__name__}: {error}'}
    row['seconds']=time.monotonic()-started
    temporary=WORK/'helper-result.tmp';temporary.write_text(json.dumps(row,indent=2));temporary.replace(WORK/'helper-result.json')
    raise SystemExit(int(row['status']!='pass'))
