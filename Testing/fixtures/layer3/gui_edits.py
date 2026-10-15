"""GUI-created lists, real enforcement and ordinary-user protected-edit rejection."""
import copy
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import time
import traceback

from accessibility import click, dump, find, texts
from browsers import start, stop, navigate as browser_navigate, page, heartbeat
from guest import WORK, host_action, rpc, run, wait
from lifecycle import finish, process_identity
from ui_controls import activate, choose, combo, edit, enabled, entries, field, focus_app, nodes, press, press_observed


def safety_paths(exclude=()):
    inventory=json.loads(run('sudo','-n','/usr/bin/python3',str(WORK/'session_inventory.py'),
        '--uid',str(os.getuid())).stdout)
    assert any(Path(row['exe']).name=='systemd' for row in inventory.values()),'Session manager missing from root-readable inventory'
    paths={row['exe'] for row in inventory.values()}
    paths.update(str(Path(p).resolve()) for p in ('/usr/bin/python3','/usr/bin/cat','/usr/bin/dash') if Path(p).exists())
    (WORK/'gui-session-inventory.json').write_text(json.dumps(inventory,indent=2))
    return sorted(paths-set(exclude))


def rejection(method, params):
    before=rpc('config_snapshot')
    try:
        rpc(method, params)
    except RuntimeError as error:
        message=str(error)
        assert 'active' in message and 'cannot be edited' in message,message
    else:
        raise AssertionError('Daemon accepted a weakening edit')
    assert rpc('config_snapshot') == before, 'Rejected edit mutated policy'
    return message


def run_cases(run_id, case, plan):
    assert os.geteuid()!=0, 'GUI/RPC journey must use the desktop user'
    selected={(row['id'],row['scenario']) for row in plan['observations']}
    base='http://web.blockuntu.test:18080'
    url=base+'/path/free'
    sessions=[]
    processes=[]

    def navigate(browser,url):
        focus_app('Firefox')
        browser_navigate(browser,url)

    def record(cid, scenario, action):
        if (cid,scenario) not in selected:
            return
        if not case(cid,action,scenario=scenario):
            (WORK/'gui-edits-failure.json').write_text(json.dumps(dump('blockuntu-gui'),indent=2))
            host_action('screenshot',name='gui-edits-failure')
            raise RuntimeError('GUI protected-edit prerequisite failed: '+cid+'/'+scenario)

    def current(kind, rule_id):
        return next(r for r in rpc('config_snapshot')['rules' if kind=='website' else 'app_rules'] if r['id']==rule_id)

    def show_identities(kind,mode):
        if kind=='application' and mode=='allowlist' and nodes(role='button',name='Show identities',showing=False):
            press_observed('Show identities', lambda:
                nodes(role='button',name='Hide identities',showing=False) and
                nodes(role='button',name='Identity',showing=False))

    def save(mode):
        press('Save')
        warning=False
        if mode=='allowlist':
            find('Save this allowlist?' if kind=='website' else 'Save this application allowlist?', 'dialog',visible=True)
            warning_text=texts()
            if kind=='application':
                assert all(term in warning_text for term in ('D-Bus','Audio','clipboard','helpers','terminal'))
            else:
                assert 'Private windows' in warning_text
            host_action('screenshot',name='gui-'+kind+'-save-warning')
            click('Save allowlist')
            warning=True
        return warning

    def create(kind,mode):
        focus_app()
        click('Websites' if kind=='website' else 'Applications')
        # Editing a long rule can leave this button above the viewport.
        # Native focus scrolls it into view before activation.
        press('New website' if kind=='website' else 'New application')
        name='Acceptance GUI '+kind+' '+mode
        edit(field('Name'),name)
        choose(combo('Tier'),'Tier 2')
        choose(combo('List behavior'),
               ('Allow only listed websites' if kind=='website' else 'Allow only listed applications')
               if mode=='allowlist' else
               ('Block listed websites' if kind=='website' else 'Block listed applications'))
        values=([base+'/free',url] if mode=='allowlist' else [base+'/path/blocked',base+'/path/blocked/child']) if kind=='website' else (
            [str(WORK/'bk-test-app-a'),str(WORK/'bk-test-app-b')] if mode=='allowlist' else ['bk-test-app-a','bk-test-block'])
        entry_name='Pattern' if kind=='website' else 'Identity' if mode=='allowlist' else 'Matcher'
        show_identities(kind,mode)
        if not entries(table=True,showing=False):
            press(entry_name)
            wait(lambda:entries(table=True,showing=False),15)
        for index,value in enumerate(values):
            if index:
                press(entry_name)
            wait(lambda:len(entries(table=True,showing=False))==index+1,15)
            choices=nodes(role='combo box',showing=False)
            choose(choices[-1], 'Exact URL' if kind=='website' else 'Path' if mode=='allowlist' else 'Command')
            # WebKit replaces the row's accessible nodes on matcher changes.
            # Wait for the replacement instead of reading the transient empty tree.
            editable=wait(lambda:entries(table=True,showing=False) if
                len(entries(table=True,showing=False))==index+1 else None,15)
            edit(editable[-1],value)
        warned=save(mode)
        rule=wait(lambda: next((r for r in rpc('config_snapshot')['rules' if kind=='website' else 'app_rules'] if r['name']==name),None),30)
        assert rule['mode']==mode and rule['tier']=='scheduled_block'
        assert len(rule['patterns' if kind=='website' else 'matchers'])==2
        return rule,warned

    def open_rule(kind,rule):
        focus_app()
        click('Websites' if kind=='website' else 'Applications')
        press(rule['name']+' Tier 2'+(' · Allowlist' if rule['mode']=='allowlist' else ''))
        wait(lambda: not enabled(field('Name')),30)
        show_identities(kind,rule['mode'])

    def activate_rule(kind,rule):
        session=rpc('start_detox',{'name':rule['name'],'duration_minutes':3,
            'site_rule_ids' if kind=='website' else 'app_rule_ids':[rule['id']]})['session']
        sessions.append(session)
        open_rule(kind,rule)
        return session

    def await_end(session):
        end=dt.datetime.fromisoformat(session['ends_at'].replace('Z','+00:00'))
        wait(lambda: dt.datetime.now(dt.timezone.utc)>=end,max(1,(end-dt.datetime.now(dt.timezone.utc)).total_seconds())+5)

    def launch():
        p=subprocess.Popen([str(WORK/'bk-test-app-b'),'--lifetime','1800'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        processes.append(p)
        wait(lambda: Path(f'/proc/{p.pid}/comm').read_text().strip()=='bk-test-app-b',5)
        return p

    def enforced(kind,rule,p):
        started=time.monotonic()
        if kind=='website':
            navigate('firefox',url)
            page('firefox','Blocked by BlocKuntu')
            from accessibility import visible_document_text
            assert rule['name'] in visible_document_text('Firefox')
            return {'visible_named_block':True}
        wait(lambda:p.poll() is not None,30)
        assert p.returncode in (0,-15,-9)
        return {'process_terminated':True,'termination_seconds':time.monotonic()-started}

    try:
        focus_app()
        if nodes(role='dialog',name='Settings'):
            click('Close Settings')
        for kind in ('website','application'):
            if kind=='website':
                start('firefox',WORK/'firefox-profile',[url])
                wait(lambda:heartbeat('firefox'),30)
                page('firefox','BlocKuntu test route /path/free')
            for mode in ('blocklist','allowlist'):
                rule,warned=create(kind,mode)
                if kind=='application' and mode=='allowlist':
                    # GUI authors the target entries; ordinary-user RPC curates
                    # current session identities before activation, as in smoke.
                    rule['matchers'] += [{'kind':'executable_path','value':p} for p in safety_paths(
                        (str(WORK/'bk-test-app-a'),str(WORK/'bk-test-app-b')))]
                    rpc('upsert_app_rule',{'rule':rule})
                    expected_values=[x['value'] for x in rule['matchers']]
                    press('Refresh')
                    def refreshed_draft():
                        button=find(rule['name']+' Tier 2 · Allowlist','button')
                        activate(button)
                        show_identities(kind,mode)
                        values=[n.queryText().getText(0,n.queryText().characterCount)
                                for n in entries(table=True,showing=False)]
                        return values==expected_values
                    wait(refreshed_draft,30)
                p=launch() if kind=='application' else None
                identity=process_identity(p) if p else None
                session=activate_rule(kind,rule)
                cid='VM-EDIT-001' if mode=='blocklist' else 'VM-EDIT-003'
                def stricter():
                    if kind=='website':
                        navigate('firefox',url)
                        page('firefox','BlocKuntu test route /path/free')
                        open_rule(kind,rule)
                    else:
                        assert p.poll() is None, 'Target was rejected before the GUI tightening edit'
                    if mode=='blocklist':
                        expected_count=len(entries(table=True,showing=False))+1
                        expected_selects=len(nodes(role='combo box',showing=False))+1
                        press('Pattern' if kind=='website' else 'Matcher')
                        wait(lambda:len(entries(table=True,showing=False))==expected_count,15)
                        choices=wait(lambda:nodes(role='combo box',showing=False) if
                            len(nodes(role='combo box',showing=False))==expected_selects else None,15)
                        choose(choices[-1],'Exact URL' if kind=='website' else 'Command')
                        editable=wait(lambda:entries(table=True,showing=False) if
                            len(entries(table=True,showing=False))==expected_count else None,15)
                        edit(editable[-1],url if kind=='website' else 'bk-test-app-b')
                    else:
                        controls=nodes(role='button',name='Remove pattern' if kind=='website' else 'Remove matcher',showing=False)
                        assert len(controls)>=2
                        activate(controls[1])
                    save(mode)
                    values_key='patterns' if kind=='website' else 'matchers'
                    target=url if kind=='website' else str(WORK/'bk-test-app-b') if mode=='allowlist' else 'bk-test-app-b'
                    wait(lambda: (any(x['value']==target for x in current(kind,rule['id'])[values_key])) == (mode=='blocklist'),30)
                    safety_retained=None
                    if kind=='application' and mode=='allowlist':
                        saved=current(kind,rule['id'])['matchers']
                        assert saved==[m for m in rule['matchers'] if m['value']!=target], 'GUI save dropped unrelated session entries'
                        safety_retained=True
                    effect=enforced(kind,rule,p)
                    open_rule(kind,rule)
                    screenshot='gui-'+cid.lower()+'-'+kind
                    host_action('screenshot',name=screenshot)
                    evidence={'rule_id':rule['id'],'kind':kind,'mode':mode,'gui_created':True,
                        'warning_confirmed':warned,'gui_save_persisted':True,'stricter_enforced':True,
                        'target_was_accessible':True,'process_identity':identity,'session':session,**effect}
                    if safety_retained is not None:
                        evidence['session_entries_preserved']=safety_retained
                    return evidence
                record(cid,kind,stricter)
                def weakening():
                    control=nodes(role='button',name='Remove pattern' if kind=='website' else 'Remove matcher',showing=False)[0] if mode=='blocklist' else find('Pattern' if kind=='website' else 'Identity','button')
                    assert not enabled(control), 'Weakening GUI control remains available'
                    edited=copy.deepcopy(current(kind,rule['id']))
                    key='patterns' if kind=='website' else 'matchers'
                    if mode=='blocklist': edited[key]=edited[key][1:]
                    else: edited[key].append({'kind':'exact_url' if kind=='website' else 'executable_path','value':url if kind=='website' else str(WORK/'bk-test-app-b')})
                    error=rejection('upsert_site_list' if kind=='website' else 'upsert_app_rule',{'rule':edited})
                    host_action('screenshot',name='gui-'+('vm-edit-002' if mode=='blocklist' else 'vm-edit-004')+'-'+kind)
                    return {'rule_id':rule['id'],'gui_control_disabled':True,'daemon_rejected':True,'policy_unchanged':True,'rejection':error}
                record('VM-EDIT-002' if mode=='blocklist' else 'VM-EDIT-004',kind,weakening)
                def protected_mode():
                    assert not enabled(combo('List behavior')) and not enabled(combo('Tier')) and not enabled(field('Name'))
                    edited=copy.deepcopy(current(kind,rule['id']))
                    edited['mode']='allowlist' if mode=='blocklist' else 'blocklist'
                    error=rejection('upsert_site_list' if kind=='website' else 'upsert_app_rule',{'rule':edited})
                    host_action('screenshot',name='gui-vm-edit-005-'+kind+'-'+mode+'-mode')
                    return {'rule_id':rule['id'],'gui_control_disabled':True,'daemon_rejected':True,'policy_unchanged':True,'rejection':error}
                record('VM-EDIT-005',kind+'-'+mode+'-mode',protected_mode)
                await_end(session)
                if p: finish(p)
            if kind=='website': stop('firefox')
    except Exception:
        (WORK/'gui-edits-failure.json').write_text(json.dumps({
            'traceback':traceback.format_exc(),'controls':dump('blockuntu-gui')},indent=2))
        host_action('screenshot',name='gui-edits-failure')
        raise
    finally:
        for p in processes: finish(p)
        stop('firefox')
