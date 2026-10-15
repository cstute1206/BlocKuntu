"""Bounded Ubuntu journeys using real clocks, browser visits and user processes."""
import datetime as dt
import json
from pathlib import Path
import subprocess
import time

from guest import WORK, host_action, rpc, run, wait


def schedule_window(now):
    # Leave at least 70 seconds to establish the 30-second allowed baseline.
    start = (now + dt.timedelta(seconds=130)).replace(second=0, microsecond=0)
    end = start + dt.timedelta(minutes=3)
    return start, end


def process_identity(process):
    path = Path(f'/proc/{process.pid}')
    return {'pid': process.pid, 'exe': str((path / 'exe').readlink()),
            'start_ticks': (path / 'stat').read_text().rsplit(')', 1)[1].split()[19]}


def launch():
    p = subprocess.Popen([str(WORK / 'bk-test-app-a'), '--lifetime', '1800'],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    wait(lambda: p.poll() is not None or
         Path(f'/proc/{p.pid}/comm').read_text().strip() == 'bk-test-app-a', 5)
    assert p.poll() is None, 'Fixture exited before readiness'
    return p


def finish(process):
    if process is not None and process.poll() is None:
        process.terminate()
        process.wait(timeout=5)


def remaining(rule_id):
    # Read-only witness of daemon-accounted usage; never write/reset the database.
    code = ('import json,sqlite3,sys; '
            'c=sqlite3.connect("file:/var/lib/blockuntu/blockuntu.sqlite3?mode=ro",uri=True); '
            'r=c.execute("SELECT remaining_seconds FROM notification_allowance_state WHERE rule_id=?",'
            '(sys.argv[1],)).fetchone(); print(json.dumps(r[0] if r else None))')
    return json.loads(run('sudo', '-n', '/usr/bin/python3', '-c', code, rule_id).stdout)


def run_cases(browser, run_id, case, selected):
    from browsers import APPS, navigate, page
    from accessibility import visible_document_text
    app = APPS[browser]
    route = f'/{browser}/longrunning'
    url = 'http://web.blockuntu.test:18080' + route

    def accessible():
        navigate(browser, url)
        page(browser, 'BlocKuntu test route ' + route, 45)

    def blocked(rule):
        page(browser, 'Blocked by BlocKuntu', 45)
        assert rule['name'] in visible_document_text(app), 'Wrong blocking rule'

    def hold_allowed(process, seconds):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            assert process.poll() is None, 'Allowed fixture died'
            assert 'BlocKuntu test route ' + route in visible_document_text(app)
            time.sleep(1)

    def transition(kind):
        prefix = f'{run_id}-{browser}-{kind}'
        # Detox uses an allowlist; schedule uses a blocklist. Both restore access.
        rule = {'id': prefix + '-site', 'name': 'Lifecycle ' + kind + ' ' + browser,
                'tier': 'scheduled_block', 'enabled': True, 'schedule_ids': [],
                'mode': 'allowlist' if kind == 'detox' else 'blocklist',
                'patterns': [{'kind': 'exact_url', 'value':
                    'http://web.blockuntu.test:18080/free' if kind == 'detox' else url}]}
        app_rule = {'id': prefix + '-app', 'name': rule['name'] + ' process',
                    'tier': 'scheduled_block', 'schedule_ids': [],
                    'matchers': [{'kind': 'command_name', 'value': 'bk-test-app-a'}]}
        rpc('upsert_site_list', {'rule': rule})
        rpc('upsert_app_rule', {'rule': app_rule})
        process = recovered = None
        evidence = {'rule_id': rule['id'], 'app_rule_id': app_rule['id'],
                    'activation': kind, 'timezone': run('date', '+%Z %z').stdout.strip()}
        try:
            process = launch()
            evidence['before_process'] = process_identity(process)
            accessible()
            if kind == 'schedule':
                start, end = schedule_window(dt.datetime.now().astimezone())
                rpc('upsert_schedule', {'schedule': {'id': prefix + '-schedule',
                    'name': rule['name'], 'windows': [{'weekday': ('mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun')[start.weekday()],
                        'start': start.strftime('%H:%M'), 'end': end.strftime('%H:%M')}]},
                    'site_rule_ids': [rule['id']], 'app_rule_ids': [app_rule['id']]})
                # End after midnight is supported by the product schedule window.
                evidence.update(starts_at=start.isoformat(), ends_at=end.isoformat())
                hold_allowed(process, 30)
                wait(lambda: dt.datetime.now().astimezone() >= start, 135)
            else:
                hold_allowed(process, 30)
                session = rpc('start_detox', {'name': rule['name'], 'duration_minutes': 3,
                    'site_rule_ids': [rule['id']], 'app_rule_ids': [app_rule['id']]})['session']
                end = dt.datetime.fromisoformat(session['ends_at'].replace('Z', '+00:00'))
                evidence['session'] = session
            activated = time.monotonic()
            # Process and browser deadlines share the activation anchor.
            wait(lambda: process.poll() is not None, 30)
            assert process.returncode in (0, -15, -9), process.returncode
            evidence['process_exit_seconds'] = time.monotonic() - activated
            blocked(rule)
            assert time.monotonic() - activated <= 45, 'Browser activation exceeded 45 seconds'
            answer = rpc('evaluate_url', {'url': url, 'probe': True})
            assert answer['decision'] == 'block' and answer['reason']['rule_id'] == rule['id']
            evidence['blocked_decision'] = answer
            host_action('screenshot', name=browser + '-p32-' + kind)
            wait(lambda: dt.datetime.now().astimezone() >= end,
                 max(1, (end - dt.datetime.now().astimezone()).total_seconds()) + 5)
            restored = time.monotonic()
            accessible()
            recovered = launch()
            evidence['after_process'] = process_identity(recovered)
            hold_allowed(recovered, 30)
            evidence.update(baseline_accessible=True, process_blocked=True,
                positive_blocked=True, restored_access=True, restored_process_survived=True,
                restore_observation_seconds=time.monotonic() - restored)
            return evidence
        finally:
            finish(process)
            finish(recovered)

    def allowance():
        prefix = f'{run_id}-{browser}-usage'
        allowance_id = prefix + '-allowance'
        rpc('upsert_allowance', {'allowance': {'id': allowance_id, 'daily_minutes': 1}})
        rule = {'id': prefix + '-site', 'name': 'Lifecycle allowance ' + browser,
                'tier': 'controlled_access', 'enabled': True, 'schedule_ids': [],
                'allowance_id': allowance_id,
                'patterns': [{'kind': 'exact_url', 'value': url}]}
        rpc('upsert_site_list', {'rule': rule})
        session = rpc('start_detox', {'name': rule['name'], 'duration_minutes': 5,
                                    'site_rule_ids': [rule['id']]})['session']
        accessible()
        initial = remaining(rule['id'])
        assert initial is None or 50 <= initial <= 60, initial
        started = time.monotonic()
        positive = wait(lambda: (value if isinstance(value := remaining(rule['id']), int)
                                and 0 < value < 60 else None), 30)
        wait(lambda: remaining(rule['id']) == 0, max(1, 120 - (time.monotonic() - started)))
        exhausted = time.monotonic()
        blocked(rule)
        answer = rpc('evaluate_url', {'url': url, 'probe': True})
        assert answer['decision'] == 'block' and answer['reason']['rule_id'] == rule['id']
        assert answer['reason']['controlled_reason'] == 'allowance_exhausted', answer
        host_action('screenshot', name=browser + '-p32-allowance')
        return {'rule_id': rule['id'], 'session': session, 'baseline_accessible': True,
                'positive_usage': True, 'remaining_after_positive_usage': positive,
                'remaining_after_exhaustion': 0, 'exhaustion_seconds': exhausted - started,
                'positive_blocked': True, 'blocked_decision': answer}

    def allowlist_unlock():
        prefix = f'{run_id}-{browser}-unlock'
        allowance_id = prefix + '-allowance'
        rpc('upsert_allowance', {'allowance': {'id': allowance_id, 'daily_minutes': 1}})
        rule = {'id': prefix + '-site', 'name': 'Lifecycle allowlist unlock ' + browser,
                'tier': 'controlled_access', 'mode': 'allowlist', 'enabled': True,
                'schedule_ids': [], 'allowance_id': allowance_id,
                'patterns': [{'kind': 'exact_url',
                              'value': 'http://web.blockuntu.test:18080/free'}]}
        rpc('upsert_site_list', {'rule': rule})
        navigate(browser, 'http://web.blockuntu.test:18080/free')
        page(browser, 'BlocKuntu test route /free')
        session = rpc('start_detox', {'name': rule['name'], 'duration_minutes': 5,
                                    'site_rule_ids': [rule['id']]})['session']
        wait(lambda: remaining(rule['id']) is not None, 30)
        until = time.monotonic() + 30
        while time.monotonic() < until:
            assert remaining(rule['id']) == 60, 'Allowed site consumed allowance'
            assert 'BlocKuntu test route /free' in visible_document_text(app)
            time.sleep(1)
        accessible()
        started = time.monotonic()
        positive = wait(lambda: value if isinstance(value := remaining(rule['id']), int)
                        and 0 < value < 60 else None, 30)
        wait(lambda: remaining(rule['id']) == 0, max(1, 120 - (time.monotonic() - started)))
        exhausted = time.monotonic()
        blocked(rule)
        answer = rpc('evaluate_url', {'url': url, 'probe': True})
        assert answer['decision'] == 'block' and answer['reason']['rule_id'] == rule['id']
        assert answer['reason']['controlled_reason'] == 'allowance_exhausted', answer
        grant = rpc('request_unlock', {'target': rule['id'],
            'reason': 'I need temporary access to complete this specific browser acceptance task'})
        start = dt.datetime.fromisoformat(grant['started_at'].replace('Z', '+00:00'))
        expires = dt.datetime.fromisoformat(grant['expires_at'].replace('Z', '+00:00'))
        assert grant['rule_id'] == rule['id'] and (expires - start).total_seconds() == 120
        accessible()
        while (left := (expires - dt.datetime.now(dt.timezone.utc)).total_seconds()) > 0:
            assert 'BlocKuntu test route ' + route in visible_document_text(app), 'Grant ended early'
            assert remaining(rule['id']) == 0, 'Grant refilled allowance'
            time.sleep(min(1, left))
        blocked(rule)
        assert remaining(rule['id']) == 0
        end = dt.datetime.fromisoformat(session['ends_at'].replace('Z', '+00:00'))
        assert dt.datetime.now(dt.timezone.utc) < end, 'Detox expired before reblocking'
        host_action('screenshot', name=browser + '-p32-unlock')
        evidence = {'rule_id': rule['id'], 'session': session, 'grant': grant,
            'grant_seconds': 120, 'allowed_only_uncharged': True, 'baseline_accessible': True,
            'positive_usage': True, 'remaining_after_positive_usage': positive,
            'remaining_after_exhaustion': 0, 'exhaustion_seconds': exhausted - started,
            'positive_blocked': True, 'blocked_decision': answer,
            'unlocked_access': True, 'reblocked_after_expiry': True}
        # Release the restrictive allowlist before running the smoke allowlist.
        wait(lambda: dt.datetime.now(dt.timezone.utc) >= end,
             max(1, (end - dt.datetime.now(dt.timezone.utc)).total_seconds()) + 5)
        accessible()
        return evidence

    actions = {'VM-ACT-001': lambda: transition('schedule'),
               'VM-ACT-002': lambda: transition('detox'), 'VM-WEB-008': allowance,
               'VM-WAL-012': allowlist_unlock}
    for cid in sorted(selected):
        if not case(cid, actions[cid]):
            raise RuntimeError('Lifecycle prerequisite failed: ' + cid)
