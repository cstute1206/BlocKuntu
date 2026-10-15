"""Ordinary-user, published-extension P4.3 observations in original profiles."""
import argparse
from layer4_platform import GUESTS, detected_guest, browsers_for, browser_paths
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import traceback
import zipfile

import accessibility
import browsers
import ui_controls
import uninstall
from guest import WORK, configure_desktop, host_action, rpc, run, wait
from layer4_browser_contract import CASES, MANIFESTS, POLICIES, fault_paths, diagnostic_witness, installed_timing, observation_seconds, crash_pause_seconds, verify_crash_pause, redact, save

BROWSERS = ('firefox', 'chrome')
RPC_TRACE = []


def available_heartbeat(browser):
    """Expected outage errors are observations, not immediate polling failure."""
    try:
        status = browsers.heartbeat(browser)
        RPC_TRACE.append({'monotonic':time.monotonic(),'browser':browser,'rpc_succeeded':True,
                          'current_session_healthy':bool(status)})
    except (OSError, RuntimeError, ValueError) as error:
        status = None
        RPC_TRACE.append({'monotonic':time.monotonic(),'browser':browser,'rpc_succeeded':False,'error':str(error)})
    save(WORK/'p43-rpc-trace.json',RPC_TRACE)
    return status


def settled_daemon(predicate):
    try:
        state = inspected()
        return state if predicate(state) else None
    except (OSError, ValueError, subprocess.CalledProcessError):
        return None


def inspected():
    return json.loads(run('sudo','-n','/usr/bin/python3',str(WORK/'browser_ops.py'),'inspect').stdout)


def collector_alive():
    state = json.loads((WORK/'p43-collector.json').read_text())
    age = (dt.datetime.now(dt.timezone.utc) - dt.datetime.fromisoformat(state['checked_at'])).total_seconds()
    return 0 <= age < 15 and state.get('collector_active') is True


def extension_code(row):
    profile = Path(row['profile'])
    eid = row['heartbeat']['extension_id']
    if row['browser'] == 'firefox':
        addons = json.loads((profile/'extensions.json').read_text())['addons']
        addon = next(a for a in addons if a['id'] == eid)
        archive = Path(addon['path'])
        if not archive.resolve().is_relative_to(profile.resolve()) or archive.suffix != '.xpi':
            raise ValueError('Installed Firefox extension archive is outside its fresh profile')
        with zipfile.ZipFile(archive) as package:
            manifest = json.loads(package.read('manifest.json'))
            names = manifest['background']['scripts']
            scripts = {name: package.read(name).decode() for name in names}
        archive_hash = hashlib.sha256(archive.read_bytes()).hexdigest()
    else:
        paths = list((profile/'Default/Extensions'/eid).glob('*/manifest.json'))
        paths = [p for p in paths if json.loads(p.read_text())['version'] == row['heartbeat']['extension_version']]
        assert len(paths) == 1, 'Expected exactly one current installed Chrome extension'
        directory = paths[0].parent
        manifest = json.loads(paths[0].read_text())
        scripts = {manifest['background']['service_worker']: (directory/manifest['background']['service_worker']).read_text()}
        archive_hash = None
    assert manifest['version'] == row['heartbeat']['extension_version'], 'Heartbeat differs from installed extension'
    timing = installed_timing('\n'.join(scripts.values()))
    return {'browser': row['browser'], 'manifest': manifest, 'archive_sha256': archive_hash,
            'timing': timing, 'scripts': scripts,
            'script_sha256': {name: hashlib.sha256(text.encode()).hexdigest() for name,text in scripts.items()}}


def observe(browser, stage):
    ui_controls.focus_app(browsers.APPS[browser])
    text = accessibility.visible_document_text(browsers.APPS[browser])
    save(WORK/('p43-'+browser+'-'+stage+'.json'), {'monotonic': time.monotonic(), 'text': text})
    host_action('screenshot', name=browser+'-'+stage)
    return text


def wait_recovered_block(browser, seconds=45):
    rule = {'name': 'P41 exact ' + browser, 'patterns': [{'value':
        'http://web.blockuntu.test:18080/' + browser + '/exact/blocked'}]}
    return uninstall.wait_named_block_page(browsers.APPS[browser], rule, seconds)


def blocked_outage(browser, seconds=25):
    browsers.page(browser, 'Blocked by BlocKuntu', seconds)
    def unavailable():
        text = accessibility.visible_document_text(browsers.APPS[browser])
        return text if ('Daemon unavailable' in text or 'Daemon heartbeat missing' in text) and 'fail-closed' in text else None
    return wait(unavailable, seconds)


def require(proof, details):
    failed = [key for key,value in proof.items() if value is not True]
    assert not failed, 'Browser checks failed: ' + ', '.join(failed) + '; ' + json.dumps(details)
    return {**proof, 'details': details}


def recovery_proof(scenario, identities, daemon_resumed, collector):
    """A declared profile relaunch must not require preserved process IDs."""
    same = all(row['recovered_pid']==row['original_pid'] for row in identities)
    restarted = all(row['recovered_pid'] not in ('0',row['original_pid']) for row in identities)
    return {'restored':daemon_resumed,'new_heartbeats':True,'same_profiles':True,
            'browser_identity_expected':restarted if scenario=='manifest' else same,
            'allowed_visible':True,'named_block_visible':True,'collector_alive':collector}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--guest', choices=GUESTS, default='ubuntu')
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--scenario', choices=tuple(CASES), required=True)
    args = parser.parse_args()
    assert args.guest == detected_guest(), 'Guest adapter does not match installed OS'
    global BROWSERS
    BROWSERS = browsers_for(args.guest)
    global POLICIES, MANIFESTS
    POLICIES, MANIFESTS = fault_paths(args.guest)
    assert 1000 <= os.getuid() < 65534
    # Reuse the proven store consent/onboarding controls, without launching GUI
    # or invoking an uninstall journey. Names/profile prefixes remain P41 and
    # are recorded verbatim; every enclosing run/clone is fresh P43.
    for name in ('click','dump','find','texts','visible_document_text'):
        setattr(uninstall,name,getattr(accessibility,name))
    for name in ('enabled','nodes'):
        setattr(uninstall,name,getattr(ui_controls,name))
    result = {'status':'running','guest':args.guest,'run_id':args.run_id,
              'scenario':args.scenario,'ordinary_uid':os.getuid(),'cases':[]}
    baseline = {}

    def checkpoint():
        save(WORK/'p43-result.json', result)

    def case(cid, action, prerequisite=True):
        row = {'id':cid,'variant':args.scenario}
        started = time.monotonic()
        try:
            if not prerequisite:
                row.update(status='blocked',reason='Failed baseline prevents this observation')
            else:
                row.update(status='pass',evidence=action())
        except Exception as error:
            row.update(status='fail',error=redact(error),traceback=redact(traceback.format_exc()))
        row['seconds'] = round(time.monotonic()-started,3)
        result['cases'].append(row); checkpoint()
        print(cid,row['status'],row.get('error',''),flush=True)
        return row['status'] == 'pass'

    checkpoint()
    try:
        def establish():
            wait(lambda: rpc('status')['enforcement_state'] == 'active',60)
            assert rpc('clock_integrity_status')['state'] == 'ok'
            configure_desktop(args.guest)
            browsers.setup(args.guest)
            result['browsers'] = [uninstall.onboard(b,args.run_id) for b in BROWSERS]
            codes = [extension_code(row) for row in result['browsers']]
            save(WORK/'p43-extensions.json',codes)
            baseline.update(inspection=inspected(),policy=rpc('config_snapshot'),
                            browsers=result['browsers'],timing={r['browser']:r['timing'] for r in codes})
            # Both existing fixture tabs are allowed before any fault.
            for browser in BROWSERS:
                ui_controls.focus_app(browsers.APPS[browser])
                host_action('keys',keys=['KEY_LEFTCTRL','KEY_1'])
                browsers.navigate(browser,'http://web.blockuntu.test:18080/free')
                browsers.page(browser,'BlocKuntu test route /free')
                observe(browser,'baseline')
            for row in result['browsers']:
                row['heartbeat'] = wait(lambda b=row['browser']:available_heartbeat(b),30)
            save(WORK/'p43-baseline.json',baseline)
            save(WORK/'p43-export.json',rpc('export_policy_toml'))
            host_action('checkpoint',name='baseline')
            return require({'both_browsers':True,'installed_timing_verified':True,'collector_alive':collector_alive(),
                            'allowed_visible':True,'named_block_visible':all(r['blocked'] for r in result['browsers'])}, {})
        ready = case('P43-BASELINE',establish)

        def injected():
            planned = {'scenario':args.scenario, 'targets': list(POLICIES if args.scenario=='policy' else MANIFESTS) if args.scenario in ('policy','manifest') else ['blockuntu.service MainPID'],
                'effective_trigger': {'policy':'replace both managed extension entries only',
                    'manifest':'replace both native executable paths, then terminate original native connections',
                    'crash':'bounded SIGSTOP pre-pause covering installed request cycle and timeout, then SIGKILL; automatic restart enabled',
                    'outage':'SIGSTOP packaged daemon MainPID; independent root SIGCONT deadline'}[args.scenario],
                'intended_duration_seconds': (baseline['policy']['strict_mode']['grace_seconds'] + 15 if args.scenario=='manifest' else
                    max(observation_seconds(v) for v in baseline['timing'].values()) if args.scenario=='outage' else crash_pause_seconds(list(baseline['timing'].values())) if args.scenario=='crash' else None),
                'automatic_restore_limit_seconds':240 if args.scenario in ('manifest','outage','crash') else None,
                'expected_state': {'policy':'both original canonical managed policy documents restored',
                    'manifest':'original native connections gone; configured strict process guard closes both unsupported-heartbeat browser sessions; reopen same profiles after restoration',
                    'crash':'new daemon identity, current-session heartbeats and functioning navigation',
                    'outage':'RPC fails; existing/new HTTP tabs fail closed before release'}[args.scenario],
                'recovery_trigger': ('automatic daemon repair/restart' if args.scenario in ('policy','crash') else
                    'restore original manifests, reopen original profiles after declared strict termination; no extension reinstall' if args.scenario=='manifest' else
                    'SIGCONT; require original-session recovery; if browsers close, reopen original profiles for independent diagnosis while preserving that automatic-recovery failure')}
            save(WORK/'p43-plan.json',planned)
            host_action('checkpoint',name='before-fault')
            host_action('fault',name=args.scenario)
            witness = json.loads((WORK/'p43-fault.json').read_text())
            assert witness['fault_witnessed'], 'Effective fault was not witnessed'
            proof = {'fault_witnessed':True}
            details = {'fault':witness}
            if args.scenario == 'policy':
                def repaired():
                    state = inspected()
                    return state if all(json.loads(state['files'][p]) == json.loads(baseline['inspection']['files'][p]) for p in POLICIES) else None
                state = wait(repaired,45)
                proof.update(policies_repaired=True)
                details['repaired'] = state
            elif args.scenario == 'crash':
                verify_crash_pause(witness, list(baseline['timing'].values()))
                state = wait(lambda: settled_daemon(lambda s:s['daemon']['pid'] != witness['before']['daemon']['pid']),30)
                healthy = {b:wait(lambda b=b: available_heartbeat(b),60) for b in BROWSERS}
                def diagnostics():
                    log = rpc('export_event_log')
                    save(WORK/'p43-diagnostics.json',log)
                    return log if all(diagnostic_witness(log['log'],b,witness['before']['checked_at'],
                        next(row['heartbeat'] for row in baseline['browsers'] if row['browser']==b)) for b in BROWSERS) else None
                events = wait(diagnostics,30)
                proof.update(daemon_restarted=True,interruption_reported=True,current_session_heartbeat=all(healthy.values()))
                details.update(restarted=state,heartbeats=healthy,
                               diagnostics=events, interpretation='Transient request-path interruption; Native Messaging processes may survive daemon restart')
            else:
                seconds = planned['intended_duration_seconds']
                failures = []
                if args.scenario == 'outage':
                    try:
                        rpc('ping')
                    except Exception as error:
                        failures.append({'at_monotonic':time.monotonic(),'error':str(error)})
                    assert failures, 'Paused daemon still answered RPC'
                health = []
                while time.monotonic() < witness['started_monotonic'] + seconds:
                    if args.scenario == 'manifest':
                        health.append({'monotonic':time.monotonic(), 'statuses': {b:rpc('extension_status',
                            {'component':browsers.BROWSER_CONFIG[b]['component']}) for b in BROWSERS}})
                        save(WORK/'p43-health-trace.json',health)
                    time.sleep(1)
                pages = {}
                if args.scenario == 'manifest':
                    strict = baseline['policy']['strict_mode']
                    assert strict['kill_supported_browser_if_extension_stale'] is True
                    for browser in BROWSERS:
                        status = rpc('extension_status', {'component':browsers.BROWSER_CONFIG[browser]['component']})
                        assert status['state']=='inactive' and status['current_session_heartbeat'] is False, status
                        assert not ui_controls.nodes(app=browsers.APPS[browser],role='frame'), 'Expected strict browser termination was not visible'
                        host_action('screenshot',name=browser+'-existing')
                    events = rpc('export_event_log')
                    save(WORK/'p43-diagnostics.json',events)
                    assert 'browser_killed_extension_stale' in events['log'], 'Missing actual strict-termination event'
                    state = inspected()
                    assert all(json.loads(state['files'][p])['path']=='/nonexistent/blockuntu-p43-native' for p in MANIFESTS)
                    proof.update(connections_terminated=witness['connections_terminated'],degraded_visible=True)
                    details.update(health_trace=health,inspection=state,diagnostics=events,
                                   strict_termination_visible=True,observed_duration_seconds=time.monotonic()-witness['started_monotonic'])
                # Preserve and observe each original first tab before opening a new one.
                for browser in BROWSERS if args.scenario=='outage' else ():
                    ui_controls.focus_app(browsers.APPS[browser])
                    host_action('keys',keys=['KEY_LEFTCTRL','KEY_1'])
                    pages[browser+'-existing'] = blocked_outage(browser)
                    observe(browser,'existing')
                    if args.scenario == 'outage':
                        host_action('keys',keys=['KEY_LEFTCTRL','KEY_T'])
                        browsers.navigate(browser,'http://web.blockuntu.test:18080/allowed')
                        pages[browser+'-new'] = blocked_outage(browser)
                        observe(browser,'new')
                if args.scenario == 'outage':
                    state = inspected()
                    details.update(pages=pages,inspection=state,rpc_failures=failures,
                                   observed_duration_seconds=time.monotonic()-witness['started_monotonic'])
                    proof.update(rpc_failed=True,beyond_installed_tolerance=True,existing_fail_closed=len(pages)==4,new_fail_closed=len(pages)==4)
                    assert state['daemon']['state'] == 'T', 'Outage auto-restored before fail-closed observations'
            proof['collector_alive'] = collector_alive()
            save(WORK/'p43-observed.json',{'proof':proof,'details':details})
            host_action('checkpoint',name='during-fault')
            return require(proof,details)
        case(CASES[args.scenario][1],injected,ready)

        def recover():
            host_action('fault',name='restore')
            state = wait(lambda: settled_daemon(lambda s:s['daemon']['state'] != 'T'),30)
            healthy = {}
            identities = []
            recovery_started_at = dt.datetime.now(dt.timezone.utc)
            fallback_reopened = []
            if args.scenario == 'manifest':
                for row in result['browsers']:
                    assert not ui_controls.nodes(app=browsers.APPS[row['browser']],role='frame'), 'Unexpected browser relaunch before restoration'
                    browsers.start(row['browser'],Path(row['profile']),['http://web.blockuntu.test:18080/free'])
            elif args.scenario == 'outage':
                # Observe the strict guard's first post-resume decision. This
                # does not turn a forced browser restart into automatic recovery.
                time.sleep(5)
                for row in result['browsers']:
                    if not ui_controls.nodes(app=browsers.APPS[row['browser']],role='frame'):
                        fallback_reopened.append(row['browser'])
                        browsers.start(row['browser'],Path(row['profile']),['http://web.blockuntu.test:18080/free'])
            for row in result['browsers']:
                browser = row['browser']
                def fresh():
                    current = available_heartbeat(browser)
                    return current if current and dt.datetime.fromisoformat(current['last_seen_at'].replace('Z','+00:00')) >= recovery_started_at else None
                healthy[browser] = wait(fresh,90)
                current_code = extension_code({**row,'heartbeat':healthy[browser]})
                original_code = next(c for c in json.loads((WORK/'p43-extensions.json').read_text()) if c['browser']==browser)
                assert (current_code['script_sha256']==original_code['script_sha256'] and current_code['manifest']==original_code['manifest']
                        and current_code['archive_sha256']==original_code['archive_sha256']), 'Installed extension changed during recovery'
                browsers.navigate(browser,'http://web.blockuntu.test:18080/free')
                browsers.page(browser,'BlocKuntu test route /free',45)
                browsers.navigate(browser,'http://web.blockuntu.test:18080/'+browser+'/exact/blocked')
                wait_recovered_block(browser)
                observe(browser,'recovered')
                identities.append({'browser':browser,'profile':row['profile'],'original_pid':row['main_pid'],
                    'recovered_pid':run('systemctl','--user','show','blockuntu-test-'+browser,'-p','MainPID','--value').stdout.strip(),
                    'extension_unchanged':True})
            proof = recovery_proof(args.scenario,identities,state['daemon']['state']!='T',collector_alive())
            save(WORK/'p43-recovery.json',{'proof':proof,'heartbeats':healthy,'inspection':state,'browser_identities':identities,
                                         'diagnostic_fallback_reopened':fallback_reopened})
            host_action('checkpoint',name='recovered')
            return require(proof,healthy)
        case('DES-BROWSER-004',recover,ready)
    except Exception as error:
        result.update(error=redact(error),traceback=redact(traceback.format_exc()))
    finally:
        result['status']='pass' if len(result['cases'])==len(CASES[args.scenario]) and all(r['status']=='pass' for r in result['cases']) else 'fail'
        if result['status'] != 'pass':
            for browser in BROWSERS:
                try:
                    observe(browser, 'failure')
                except Exception as error:
                    save(WORK/('p43-'+browser+'-failure.json'), {'error': redact(error)})
        checkpoint()


if __name__ == '__main__':
    main()
