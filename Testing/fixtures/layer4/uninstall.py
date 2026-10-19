"""Ordinary-user native P4.1 journeys, using actual GUI and store browsers."""
import argparse
from layer4_platform import GUESTS, detected_guest, browsers_for, browser_paths, resolution_witness, blocking_resolution
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import threading
import time
import traceback

from guest import WORK, configure_desktop, host_action, rpc, run, wait
from layer4_uninstall_contract import CASES, cases_for, firefox_sentinel_preservation, native_purge_witness, native_removal_witness, handoff_heartbeats, installed_packages_preserved, redact, save


def no_lease():
    return not Path('/run/blockuntu/package-removal-lease').exists()


def inspected():
    return json.loads(run('sudo', '-n', '/usr/bin/python3', str(WORK / 'uninstall_ops.py'), 'inspect').stdout)


def collector_alive():
    state = json.loads((WORK / 'p41-collector.json').read_text())
    age = (dt.datetime.now(dt.timezone.utc) - dt.datetime.fromisoformat(state['checked_at'])).total_seconds()
    return 0 <= age < 20 and state['collector']['stdout'].strip() == 'active'


def functional_baseline():
    wait(lambda: '# BEGIN BLOCKUNTU MANAGED' in Path('/etc/hosts').read_text() and
         'p41-blocked.blockuntu.test' in Path('/etc/hosts').read_text(), 45)
    resolution = resolution_witness('p41-blocked.blockuntu.test', run)
    assert blocking_resolution(resolution), resolution
    control = resolution_witness('p41-unrelated.blockuntu.test', run)
    assert control['native']['returncode'] == 0 and control['native']['addresses'] == ['127.0.0.42'], control
    near = subprocess.Popen([str(WORK / 'bk-test-app-b'), '--lifetime', '180'],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    blocked = subprocess.Popen([str(WORK / 'bk-test-block'), '--lifetime', '180'],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        wait(lambda: blocked.poll() is not None, 30)
        assert blocked.returncode in (0, -15, -9), blocked.returncode
        assert near.poll() is None, 'Unrelated fixture was terminated'
        return {'hosts_blocked': True, 'process_blocked': True, 'near_miss_alive': True,
                'blocked_pid': blocked.pid, 'exit_code': blocked.returncode, 'near_miss_pid': near.pid,
                'resolution': resolution, 'control_resolution': control}
    finally:
        for process in (near, blocked):
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


def fresh_protected():
    import pyatspi
    def active():
        return any(n.getState().contains(pyatspi.STATE_ACTIVE) for n in nodes(role='frame'))
    if not active():
        host_action('focus_gui')
        wait(active, 15)
    if nodes(role='dialog', name='Settings'):
        click('Close Settings')
    press('Refresh')
    open_settings('Protected changes and uninstall')


def type_phrase(slot):
    node = wait(lambda: next((n for n in nodes(role='password text', showing=False)
                             if n.name.startswith('Uninstall phrase')), None), 15)
    assert enabled(node) and node.queryComponent().grabFocus()
    host_action('type_credential', slot=slot)
    count = json.loads((WORK / 'host-action-done.json').read_text())['typed_characters']
    assert 1 <= count <= 200
    wait(lambda: node.queryText().characterCount == count, 15)


def press_uninstall():
    """Native activation requires observed keyboard focus, not its acknowledgement."""
    import pyatspi
    node = wait(lambda: next(iter(nodes(role='button', name='Uninstall BlocKuntu')), None), 15)
    assert enabled(node), 'Uninstall button is disabled'
    assert node.queryComponent().grabFocus(), 'Uninstall button refused focus'
    wait(lambda: node.getState().contains(pyatspi.STATE_FOCUSED), 15)
    host_action('keys', keys=['KEY_ENTER'])


def select_uninstall_chrome_store(browser):
    """Fresh consent can rerender or redirect; use observed native tab focus."""
    import pyatspi
    from browsers import APPS, STORES, navigate
    from ui_controls import focus_app
    app = APPS[browser]
    focus_app(app)
    host_action('keys', keys=['KEY_LEFTCTRL', 'KEY_2'])
    navigate(browser, STORES[browser])

    def surface():
        text = visible_document_text(app)
        return ('consent' if 'Before you continue to Google' in text else
                'store' if 'BlocKuntu - Chrome Web Store' in text else None)

    if wait(surface, 60) == 'store':
        return
    find('Reject all', 'button', app, seconds=15, visible=False)
    for _ in range(40):
        if surface() == 'store':
            return
        # Requery after each native focus change: Chrome can replace this node.
        button = next((n for n in nodes(app=app, role='button', name='Reject all', showing=False)), None)
        if button and enabled(button) and all(button.getState().contains(s) for s in
                                            (pyatspi.STATE_FOCUSED, pyatspi.STATE_SHOWING)):
            host_action('keys', keys=['KEY_ENTER'])
            wait(lambda: surface() == 'store', 60)
            return
        host_action('keys', keys=['KEY_TAB'])
    raise AssertionError('Store consent native focus was not observed')


def wait_named_block_page(app, rule, seconds=45):
    """Observe the actual policy page, allowing bounded automatic recovery."""
    expected = ('Blocked by BlocKuntu', rule['name'], rule['patterns'][0]['value'])
    last_text = ''
    def ready():
        nonlocal last_text
        last_text = visible_document_text(app)
        return last_text if all(value in last_text for value in expected) else None
    try:
        return wait(ready, seconds)
    except TimeoutError as error:
        raise TimeoutError('No named block page for ' + rule['name'] + ': ' +
                           last_text[:400]) from error


def onboard(browser, run_id):
    from browsers import (APPS, BROWSER_CONFIG, STORES, start, optional_click, select_tab,
                          add_from_chrome_store, confirm_install,
                          heartbeat, firefox_managed_store_source, navigate, page)
    app = APPS[browser]
    profile = WORK / ('p41-' + browser + '-profile')
    profile.mkdir(exist_ok=False)
    if browser == 'firefox':
        (profile / 'user.js').write_text('user_pref("browser.shell.checkDefaultBrowser", false);\n')
    start(browser, profile, ['http://web.blockuntu.test:18080/free', STORES[browser]])
    # Fedora starts its fresh GNOME session in overview. Native permission
    # doorhangers are unavailable until the browser is actually foregrounded.
    from ui_controls import focus_app
    focus_app(app)
    if browser == 'firefox':
        optional_click('Continue', app, seconds=15)
        select_tab(browser, lambda name: 'BlocKuntu' in name and 'test route' not in name)
        try:
            find('Add to Firefox', 'link', app, seconds=5, visible=True)
        except TimeoutError:
            find('Remove', 'link', app, seconds=30, visible=True)
            current = wait(lambda: heartbeat(browser), 30)
            source = firefox_managed_store_source(profile, current['extension_id'])
        else:
            click('Add to Firefox', app=app, role='link')
            confirm_install('Add', app)
            optional_click('OK', app, seconds=3)
            source = {'installation_path': 'store_prompt'}
    else:
        select_uninstall_chrome_store(browser)
        add_from_chrome_store(browser, app)
        confirm_install('Add extension', app)
        source = {'installation_path': 'store_prompt'}
    status = wait(lambda: heartbeat(browser), 30)
    navigate(browser, 'http://web.blockuntu.test:18080/free')
    page(browser, 'BlocKuntu test route /free')
    rule = {'id': run_id + '-' + browser, 'name': 'P41 exact ' + browser,
            'tier': 'hard', 'enabled': True, 'schedule_ids': [],
            'patterns': [{'kind': 'exact_url', 'value': 'http://web.blockuntu.test:18080/' + browser + '/exact/blocked'}]}
    rpc('upsert_site_list', {'rule': rule})
    navigate(browser, rule['patterns'][0]['value'])
    wait_named_block_page(app, rule)
    # Keep the original process and profile alive through uninstall.
    main = run('systemctl', '--user', 'show', 'blockuntu-test-' + browser, '-p', 'MainPID', '--value').stdout.strip()
    return {'browser': browser, 'profile': str(profile), 'main_pid': main,
            'browser_version': run(BROWSER_CONFIG[browser]['binary'], '--version').stdout.strip(),
            'package_source': BROWSER_CONFIG[browser]['package_source'], 'store_url': STORES[browser],
            'heartbeat': status, 'blocked': True, 'allowed': True, **source}



def probe_native_gui(label):
    """A rejected package transaction must preserve a working GUI uninstall path."""
    from ui_controls import focus_app
    unit = 'blockuntu-p41-gui-' + label
    assert not nodes(role='frame'), 'A preexisting GUI would mask missing runtime dependencies'
    run('systemd-run', '--user', '--collect', '--unit=' + unit,
        '--property=RuntimeMaxSec=90', '/usr/bin/blockuntu-gui')
    proof = {'unit': unit, 'label': label, 'ordinary_uid': os.getuid()}
    try:
        wait(lambda: nodes(role='dialog', name='Welcome to BlocKuntu') or
             nodes(role='button', name='Settings'), 30)
        focus_app()
        if nodes(role='dialog', name='Welcome to BlocKuntu'):
            click('Get started')
            wait(lambda: not nodes(role='dialog', name='Welcome to BlocKuntu'), 15)
        click('Settings')
        find('Settings', 'dialog', seconds=15)
        click('Close Settings')
        wait(lambda: not nodes(role='dialog', name='Settings'), 15)
        proof.update(gui_restart_healthy=True, settings_opened=True, settings_closed=True)
        return proof
    finally:
        proof['journal'] = redact(run('journalctl', '--user', '-b', '--no-pager', '-u', unit, check=False).stdout)
        save(WORK / ('p41-gui-' + label + '.json'), proof)
        run('systemctl', '--user', 'stop', unit, check=False)
        wait(lambda: not nodes(role='frame'), 15)

def main():
    global click, dump, find, texts, visible_document_text
    global combo, enabled, nodes, open_settings, press
    parser = argparse.ArgumentParser()
    parser.add_argument('--guest', choices=GUESTS, default='ubuntu')
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--scenario', choices=tuple(CASES), required=True)
    args = parser.parse_args()
    assert args.guest == detected_guest(), 'Guest adapter does not match installed OS'
    global BROWSERS
    BROWSERS = browsers_for(args.guest)
    selected_cases = cases_for(args.guest)
    if args.scenario == 'authorized' or args.guest != 'ubuntu':
        from accessibility import click, dump, find, texts, visible_document_text
        from ui_controls import combo, enabled, nodes, open_settings, press
    assert 1000 <= os.getuid() < 65534, 'GUI and RPC checks must be ordinary-user operations'
    result = {'status': 'running', 'guest': args.guest, 'scenario': args.scenario,
              'run_id': args.run_id, 'ordinary_uid': os.getuid(), 'cases': []}

    def checkpoint():
        save(WORK / 'p41-result.json', result)

    def case(cid, action, prerequisite=True):
        started = time.monotonic()
        row = {'id': cid, 'variant': args.scenario}
        if not prerequisite:
            row.update(status='blocked', reason='Earlier prerequisite prevented this observation')
        else:
            try:
                row.update(status='pass', evidence=action())
            except Exception as error:
                row.update(status='fail', error=redact(error), traceback=redact(traceback.format_exc()))
        row['seconds'] = round(time.monotonic() - started, 3)
        result['cases'].append(row)
        checkpoint()
        print(cid, row['status'], flush=True)
        return row['status'] == 'pass'

    checkpoint()
    try:
        def baseline():
            wait(lambda: rpc('status')['enforcement_state'] == 'active', 60)
            rpc('upsert_site_list', {'rule': {'id': args.run_id + '-hosts', 'name': 'P41 hosts',
                'tier': 'hard', 'enabled': True, 'schedule_ids': [],
                'patterns': [{'kind': 'domain', 'value': 'p41-blocked.blockuntu.test'}]}})
            rpc('upsert_app_rule', {'rule': {'id': args.run_id + '-process', 'name': 'P41 process',
                'tier': 'hard', 'matchers': [{'kind': 'command_name', 'value': 'bk-test-block'}]}})
            proof = functional_baseline()
            state = inspected()
            assert state['package']['stdout'].startswith('installed '), state['package']
            clock = rpc('clock_integrity_status')
            assert clock['state'] == 'ok' and clock['boot_id'] != 'test'
            assert collector_alive(), 'Independent collector is not healthy'
            if args.scenario != 'authorized' and args.guest != 'ubuntu':
                configure_desktop(args.guest)
                result['gui_baseline'] = probe_native_gui('baseline')
            if args.scenario == 'authorized':
                address = run('getent', 'ahostsv4', 'web.blockuntu.test').stdout.split()
                assert address and address[0] == '127.0.0.1', 'Local browser fixture DNS is not configured'
                configure_desktop(args.guest)
                run('systemd-run', '--user', '--collect', '--property=ExitType=cgroup',
                    '--unit=blockuntu-acceptance-gui', 'gtk-launch', 'local.blockuntu.gui')
                find('Welcome to BlocKuntu', 'dialog', seconds=60)
                click('Get started')
                wait(lambda: not nodes(role='dialog', name='Welcome to BlocKuntu'), 30)
                from browsers import setup
                setup(args.guest)
                result['browsers'] = [onboard(b, args.run_id) for b in BROWSERS]
                result['baseline_browser_count'] = len(result['browsers'])
                fresh_protected()
                host_action('screenshot', name='p41-baseline')
            save(WORK / 'p41-baseline.json', {'policy': rpc('config_snapshot'), 'inspection': state,
                                            'clock': clock, 'functional': proof, 'browsers': result.get('browsers', [])})
            save(WORK / 'p41-export.json', rpc('export_policy_toml'))
            host_action('checkpoint', name='baseline')
            return {**proof, 'installed': True, 'clock_healthy': True, 'collector_alive': True}

        ready = case('P41-BASELINE', baseline)
        if args.scenario != 'authorized':
            def rejected_removal():
                assert no_lease(), 'Unexpected initial removal lease'
                host_action('fault', name='direct-' + args.scenario)
                answer = json.loads((WORK / 'host-action-done.json').read_text())
                assert answer['returncode'] != 0 and answer['refusal_observed'], 'Package command did not refuse removal'
                state = inspected()
                assert state['package']['stdout'].startswith('installed '), state['package']
                assert rpc('status')['enforcement_state'] == 'active'
                assert no_lease()
                proof = functional_baseline()
                save(WORK / 'p41-after.json', state)
                if args.guest != 'ubuntu':
                    before = json.loads((WORK / 'p41-baseline.json').read_text())['inspection']
                    assert installed_packages_preserved(before['package_inventory'], state['package_inventory']), 'Rejected transaction changed installed packages'
                    proof['all_packages_preserved'] = True
                    result['gui_post_refusal'] = probe_native_gui('post-refusal')
                    proof['gui_restart_healthy'] = True
                host_action('checkpoint', name='after-uninstall')
                return {**proof, 'command_rejected': True, 'package_installed': True, 'no_lease': True,
                        'command_returncode': answer['returncode'], 'package': state['package']}
            case(selected_cases[args.scenario][1], rejected_removal, ready)
        else:
            def wrong_phrase():
                before = rpc('config_snapshot')
                type_phrase('invalid')
                press_uninstall()
                wait(lambda: 'The uninstall phrase does not match.' in texts(), 30)
                assert no_lease() and rpc('config_snapshot') == before
                return {'gui_wrong_phrase_rejected': True, 'no_lease': True, 'policy_unchanged': True}
            ready = case('DES-UNINSTALL-004', wrong_phrase, ready)
            if ready:
                rpc('set_protected_access_mode', {'mode': 'no_active_schedule_or_detox'})
                scheduled = {'id': args.run_id + '-scheduled', 'name': 'P41 schedule',
                    'tier': 'scheduled_block', 'enabled': True, 'schedule_ids': [],
                    'patterns': [{'kind': 'exact_url', 'value': 'http://web.blockuntu.test:18080/free'}]}
                rpc('upsert_site_list', {'rule': scheduled})
                now = dt.datetime.now().astimezone()
                start = now.replace(second=0, microsecond=0)
                end = start + dt.timedelta(minutes=3)
                assert end.date() == start.date(), 'Run schedule selection away from local midnight'
                schedule = rpc('upsert_schedule', {'schedule': {'id': args.run_id + '-schedule',
                    'name': 'P41 schedule', 'windows': [{'weekday': ('mon','tue','wed','thu','fri','sat','sun')[now.weekday()],
                    'start': start.strftime('%H:%M'), 'end': end.strftime('%H:%M')}]},
                    'site_rule_ids': [scheduled['id']]})
                result['schedule'] = {'end': end.isoformat(), 'response': schedule}

            def protected_rejection(kind):
                wait(lambda: not rpc('tier1_edit_status')['protected_access_open'], 30)
                fresh_protected()
                assert not enabled(combo('Protected settings access')), 'Protected access selector remains editable'
                type_phrase('uninstall')
                press_uninstall()
                wait(lambda: 'only available while no schedule or Detox is active' in texts(), 30)
                try:
                    rpc('prepare_uninstall')
                except RuntimeError as error:
                    assert 'GUI uninstall' in str(error) and 'only available' in str(error), redact(error)
                else:
                    raise AssertionError('RPC prepared uninstall during protected activation')
                assert no_lease()
                return {kind + '_active': True, 'gui_uninstall_rejected': True, 'settings_locked': True,
                        'rpc_uninstall_rejected': True, 'no_lease': True, 'checked_at': dt.datetime.now().astimezone().isoformat()}
            ready = case('DES-PROTECT-001', lambda: protected_rejection('schedule'), ready)
            if ready:
                wait(lambda: dt.datetime.now().astimezone() >= end and rpc('tier1_edit_status')['protected_access_open'], 190)
                session = rpc('start_detox', {'name': 'P41 protected Detox', 'duration_minutes': 1,
                                              'site_rule_ids': [scheduled['id']]})['session']
                result['detox'] = session
            ready = case('DES-PROTECT-002', lambda: protected_rejection('detox'), ready)

            def idle():
                expires = dt.datetime.fromisoformat(result['detox']['ends_at'])
                status = wait(lambda: rpc('tier1_edit_status') if dt.datetime.now(dt.timezone.utc) >= expires and
                              rpc('tier1_edit_status')['protected_access_open'] else None, 100)
                fresh_protected()
                wait(lambda: enabled(combo('Protected settings access')), 30)
                clock = rpc('clock_integrity_status')
                assert clock['state'] == 'ok'
                return {'natural_expiry': True, 'no_active_sessions': True, 'protected_access_open': True,
                        'settings_available': True, 'clock_healthy': True, 'clock': clock, 'status': status}
            ready = case('DES-PROTECT-003', idle, ready)

            def uninstall():
                type_phrase('uninstall')
                before = inspected()
                save(WORK / 'p41-before-uninstall.json', before)
                host_action('checkpoint', name='before-uninstall')
                samples = []
                finished = threading.Event()
                def observe():
                    while not finished.is_set():
                        try:
                            state = rpc('status')['enforcement_state']
                            sample = {'state': state, 'at': dt.datetime.now(dt.timezone.utc).isoformat(),
                                      'browsers': {b: rpc('extension_status', {'component': b + '_extension'})
                                                   for b in BROWSERS}}
                            if state == 'uninstalling':
                                sample['handoff'] = json.loads(run('sudo', '-n', '/usr/bin/python3', str(WORK / 'uninstall_ops.py'), 'handoff').stdout)
                            samples.append(sample)
                            save(WORK / 'p41-handoff.json', samples)
                        except (RuntimeError, OSError, ValueError):
                            pass
                        finished.wait(0.2)
                watcher = threading.Thread(target=observe, daemon=True)
                watcher.start()
                try:
                    requested_at = dt.datetime.now().astimezone().isoformat()
                    result['uninstall_requested_at'] = requested_at
                    press_uninstall()
                    def removed():
                        state = inspected()
                        return state if state['package']['returncode'] != 0 and not Path('/usr/bin/blockuntud').exists() else None
                    state = wait(removed, 120)
                    result['removed_monotonic'] = time.monotonic()
                finally:
                    finished.set()
                    watcher.join(timeout=7)
                witnessed = [s for s in samples if s['state'] == 'uninstalling']
                assert witnessed, 'Did not observe genuine uninstall handoff'
                notified = handoff_heartbeats(samples, BROWSERS)
                for browser in BROWSERS:
                    assert notified[browser], 'No post-handoff heartbeat: ' + browser
                wait(lambda: not nodes(role='frame'), 30)
                assert collector_alive(), 'Collector did not survive uninstall'
                save(WORK / 'p41-after.json', state)
                host_action('checkpoint', name='after-uninstall')
                logs = json.loads(run('sudo', '-n', '/usr/bin/python3', str(WORK / 'uninstall_ops.py'), 'native-log').stdout)
                witness = native_removal_witness(logs, requested_at, before['package']['stdout'].split()[-1], args.guest)
                assert witness, 'Current native removal missing from native package transaction logs'
                return {'gui_initiated': True, 'uninstalling_witnessed': True, 'both_extensions_notified': True,
                        ('native_dpkg_purge' if args.guest == 'ubuntu' else 'native_package_removal'): True, 'package_absent': True, 'gui_closed': True, 'collector_alive': True,
                        'notification_witness': 'real post-handoff heartbeats; browser behavior checked separately', 'native_transaction': witness}
            removed = case('DES-UNINSTALL-005', uninstall, ready)
            # A notice/log/GUI-close check can fail after real physical removal.
            # Preserve that failure and still observe independent postconditions.
            post_removal = result.get('removed_monotonic') is not None

            def cleanup():
                state = inspected()
                save(WORK / 'p41-cleanup.json', state)
                remaining = [p for p, exists in state['exists'].items() if exists and p not in (
                    '/etc/firefox/policies/policies.json', browser_paths(args.guest)[0][1].replace('blockuntu.json','p41-unrelated.json'))]
                seeded = json.loads((WORK / 'p41-seeded.json').read_text())
                missing_hosts = [line for line in seeded['hosts'].splitlines() if line not in state['hosts'].splitlines()]
                parse_errors = {}
                def policy_json(field):
                    try:
                        return json.loads(state[field] or '{}')
                    except ValueError as error:
                        parse_errors[field] = str(error)
                        return {}
                policy = policy_json('firefox_policy')
                chrome = policy_json('chromium_sentinel')
                before = json.loads((WORK / 'p41-before-uninstall.json').read_text())
                firefox_preservation = firefox_sentinel_preservation(before['firefox_policy'], state['firefox_policy'])
                proof = {'managed_state_removed': not remaining and not state['native_manifests'] and
                         not state['user_native_copies'] and 'ActiveState=active' not in state['units']['stdout'] and
                         'LoadState=loaded' not in state['units']['stdout'] and 'BLOCKUNTU MANAGED' not in state['hosts'] and
                         'blockuntu' not in json.dumps(policy).lower(),
                         'unrelated_hosts_preserved': not missing_hosts and
                         '127.0.0.42 p41-unrelated.blockuntu.test' in state['hosts'],
                         'firefox_setting_preserved': firefox_preservation['status'] == 'preserved',
                         'chromium_policy_preserved': chrome == {'ShowHomeButton': True}}
                findings = {'proof': proof, 'firefox_preservation': firefox_preservation,
                            'remaining_managed_paths': remaining,
                            'remaining_native_manifests': state['native_manifests'],
                            'remaining_user_native_copies': state['user_native_copies'],
                            'missing_hosts_rows': missing_hosts, 'policy_parse_errors': parse_errors}
                save(WORK / 'p41-cleanup-findings.json', findings)
                failed = [key for key, value in proof.items() if not value]
                assert not failed, 'Cleanup checks failed: ' + ', '.join(failed) + '; ' + json.dumps(findings)
                return {**proof, 'inspection': state}
            case('DES-UNINSTALL-006', cleanup, post_removal)

            def recovered_browsers():
                from browsers import navigate, page, APPS
                timeout = max(75, *(r['heartbeat']['heartbeat_timeout_seconds'] for r in result['browsers'])) + 30
                until = result['removed_monotonic'] + timeout
                while time.monotonic() < until:
                    time.sleep(min(1, until - time.monotonic()))
                observations = {}
                result['browser_recovery'] = observations
                for row in result['browsers']:
                    browser = row['browser']
                    main = run('systemctl', '--user', 'show', 'blockuntu-test-' + browser, '-p', 'MainPID', '--value').stdout.strip()
                    observed = {'same_process': main == row['main_pid'] and main != '0',
                                'main_pid': main, 'management_accessible': False, 'http_accessible': False, 'errors': {}}
                    observations[browser] = observed
                    if observed['same_process']:
                        try:
                            navigate(browser, 'about:addons' if browser == 'firefox' else 'chrome://extensions')
                            if browser == 'firefox':
                                # about:addons can default to its discovery view.
                                page(browser, 'Add-ons Manager', 45)
                                click('Extensions', app='Firefox', role='page tab')
                                import pyatspi
                                wait(lambda: any(n.getState().contains(pyatspi.STATE_SELECTED) for n in
                                     nodes(app='Firefox', role='page tab', name='Extensions')), 15)
                            else:
                                find('Extensions', 'document web', APPS[browser], seconds=45, visible=True)
                            observed['management_accessible'] = True
                        except Exception as error:
                            observed['errors']['management'] = redact(error)
                        try:
                            navigate(browser, 'http://web.blockuntu.test:18080/' + browser + '/exact/blocked')
                            page(browser, 'BlocKuntu test route /' + browser + '/exact/blocked', 45)
                            observed['http_accessible'] = True
                        except Exception as error:
                            observed['errors']['http'] = redact(error)
                        host_action('screenshot', name='p41-' + browser + '-recovered')
                    else:
                        observed['errors']['process'] = 'Original browser process disappeared; no replacement launched'
                    save(WORK / 'p41-browser-recovery.json', observations)
                proof = {'same_browsers_alive': all(o['same_process'] for o in observations.values()),
                         **{b + '_' + kind + '_accessible': observations[b][kind + '_accessible']
                            for b in BROWSERS for kind in ('management', 'http')},
                         'past_failure_tolerance': True}
                assert all(proof.values()), 'Browser recovery failed: ' + json.dumps(observations)
                return {**proof,
                        'post_removal_seconds': time.monotonic() - result['removed_monotonic'], 'required_seconds': timeout}
            case('DES-UNINSTALL-007', recovered_browsers, post_removal)
    except Exception as error:
        result['error'] = redact(error)
        result['traceback'] = redact(traceback.format_exc())
    finally:
        done = {r['id'] for r in result['cases']}
        for cid in selected_cases[args.scenario]:
            if cid not in done:
                result['cases'].append({'id': cid, 'variant': args.scenario, 'status': 'blocked', 'reason': 'Journey prerequisite failed'})
        result['status'] = 'pass' if all(r['status'] == 'pass' for r in result['cases']) else 'fail'
        if result['status'] != 'pass' and (args.scenario == 'authorized' or args.guest != 'ubuntu'):
            try:
                save(WORK / 'p41-gui-failure.json', dump('blockuntu-gui'))
                save(WORK / 'p41-firefox-failure.json', dump('Firefox'))
                from browsers import APPS
                save(WORK / ('p41-' + BROWSERS[1] + '-failure.json'), dump(APPS[BROWSERS[1]]))
            except Exception:
                pass
        checkpoint()


if __name__ == '__main__':
    main()
