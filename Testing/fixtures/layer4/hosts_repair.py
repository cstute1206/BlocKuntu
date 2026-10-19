"""Ordinary-user policy and actual NSS/HTTP witnesses for native P4.2."""
import argparse
from layer4_platform import GUESTS, detected_guest, browsers_for, browser_paths, resolution_witness, blocking_resolution, wait_hosts_mapping
import datetime as dt
import json
import os
from pathlib import Path
import time
import traceback
import urllib.error
import urllib.request

from guest import WORK, host_action, rpc, run, wait
from layer4_hosts_contract import (APPEND_SENTINEL, CASES, FAULTS, FREE, HARD,
                                   OUTSIDE, SCHEDULED, domain_rows, expected_rows,
                                   external_rows, redact, save, split_hosts)

HARD_DOMAINS = (HARD, 'www.' + HARD)


def schedule_observation_in_window(stage, start, end):
    """Check the measurement interval, independently of later evidence copying."""
    began = dt.datetime.fromisoformat(stage['observation_started_at'])
    finished = dt.datetime.fromisoformat(stage['at'])
    assert began <= finished, 'Schedule observation timestamps are reversed'
    if stage['stage'] == 'inactive':
        assert finished < start, 'Missed initial inactive observation'
    elif stage['stage'] == 'active':
        assert start <= began <= finished < end, 'Missed active schedule observation'
    elif stage['stage'] == 'ended':
        assert end <= began, 'Missed expired schedule observation'
    else:
        raise AssertionError('Unknown schedule observation stage')


def inspected():
    return json.loads(run('sudo', '-n', '/usr/bin/python3', str(WORK / 'hosts_ops.py'), 'inspect').stdout)


def addresses(name):
    answer = run('getent', 'ahostsv4', name, check=False, timeout=5)
    return sorted({line.split()[0] for line in answer.stdout.splitlines() if line.split()})


def http(name):
    url = 'http://' + name + ':18080/free'
    try:
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(url, timeout=3) as response:
            body = response.read(8192).decode()
            return {'accessible': response.status == 200 and 'BlocKuntu test route /free' in body,
                    'status': response.status, 'url': url}
    except (urllib.error.URLError, OSError) as error:
        return {'accessible': False, 'error': str(error), 'url': url}


def collector_alive():
    state = json.loads((WORK / 'p42-collector.json').read_text())
    age = (dt.datetime.now(dt.timezone.utc) - dt.datetime.fromisoformat(state['checked_at'])).total_seconds()
    return 0 <= age < 15 and state['collector']['stdout'].strip() == 'active'


def immutable(state):
    answer = state['hosts_attributes']
    fields = answer['stdout'].split()
    return answer['returncode'] == 0 and bool(fields) and 'i' in fields[0]


def ready_state(domains, seconds=30):
    def compliant():
        state = inspected()
        try:
            block, outside = split_hosts(state['hosts'] or '')
            if domain_rows(block) == expected_rows(domains) and immutable(state):
                return state
        except ValueError:
            pass
        return None
    return wait(compliant, seconds)


def require(proof, details):
    failed = [key for key, value in proof.items() if value is not True]
    assert not failed, 'Hosts checks failed: ' + ', '.join(failed) + '; ' + json.dumps(details)
    return {**proof, 'details': details}


def connectivity(domains):
    names = [*domains, FREE, 'near-' + HARD]
    observations = {}
    for name in names:
        resolution = resolution_witness(name, run)
        observations[name] = {'addresses': resolution['native']['addresses'], 'resolution': resolution, 'http': http(name)}
    blocked = all(blocking_resolution(observations[name]['resolution']) for name in domains)
    denied = all(not observations[name]['http']['accessible'] for name in domains)
    controls = all(observations[name]['addresses'] == ['127.0.0.2'] and observations[name]['http']['accessible']
                   for name in (FREE, 'near-' + HARD))
    return {'hard_resolution_blocked': blocked, 'hard_http_blocked': denied,
            'control_http_accessible': controls}, observations


def wait_connectivity(domains, seconds=30):
    """Require normal DNS and real HTTP to agree after hosts changes."""
    attempts = []
    def compliant():
        proof, details = connectivity(domains)
        attempts.append({'at': dt.datetime.now(dt.timezone.utc).isoformat(),
                         'proof': proof, 'details': details})
        save(WORK / 'p42-connectivity-settling.json', attempts)
        return (proof, details) if all(proof.values()) else None
    return wait(compliant, seconds)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--guest', choices=GUESTS, default='ubuntu')
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--scenario', choices=tuple(CASES), required=True)
    args = parser.parse_args()
    assert args.guest == detected_guest(), 'Guest adapter does not match installed OS'
    global BROWSERS
    BROWSERS = browsers_for(args.guest)
    assert 1000 <= os.getuid() < 65534, 'Policy/function checks must be ordinary-user operations'
    result = {'status': 'running', 'guest': args.guest, 'scenario': args.scenario,
              'run_id': args.run_id, 'ordinary_uid': os.getuid(), 'cases': []}
    baseline = {}

    def checkpoint():
        save(WORK / 'p42-result.json', result)

    def case(cid, action, prerequisite=True):
        row = {'id': cid, 'variant': args.scenario}
        started = time.monotonic()
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
        def establish():
            wait(lambda: rpc('status')['enforcement_state'] == 'active', 60)
            wait(lambda: (WORK / 'p42-collector.json').exists(), 15)
            wait(lambda: addresses(FREE) == ['127.0.0.2'] and http(FREE)['accessible'], 30)
            original = rpc('config_snapshot')
            assert not original['rules'], 'Exact candidate baseline must contain no website rules'
            domains = () if args.scenario == 'allowlist' else HARD_DOMAINS
            if domains:
                rpc('upsert_site_list', {'rule': {'id': args.run_id + '-hard', 'name': 'P42 persistent Tier 1',
                    'tier': 'hard', 'mode': 'blocklist', 'enabled': True, 'schedule_ids': [],
                    'patterns': [{'kind': 'domain', 'value': HARD, 'match_subdomains': True}]}})
            state = ready_state(domains)
            clock = rpc('clock_integrity_status')
            health = rpc('enforcement_status')['hosts_file']
            assert health['immutable_required'], 'Installed default must require immutable hosts'
            function, samples = wait_connectivity(domains)
            proof = {**function, 'installed': state['package']['stdout'].startswith('installed '), 'clock_healthy': clock['state'] == 'ok',
                     'managed_exact': True, 'immutable_enabled': immutable(state), 'collector_alive': collector_alive(),
                     'control_http_accessible': function['control_http_accessible']}
            if domains:
                assert function['hard_resolution_blocked'] and function['hard_http_blocked'], samples
            baseline.update(policy=rpc('config_snapshot'), inspection=state, clock=clock, connectivity=samples, health=health)
            save(WORK / 'p42-baseline.json', baseline)
            save(WORK / 'p42-export.json', rpc('export_policy_toml'))
            host_action('checkpoint', name='baseline')
            return require(proof, samples)

        ready = case('P42-BASELINE', establish)
        if args.scenario != 'allowlist':
            cid = {'remove': 'DES-HOSTS-001', 'modify': 'DES-HOSTS-001', 'append': 'DES-HOSTS-002', 'delete': 'DES-HOSTS-003'}[args.scenario]
            def repaired():
                host_action('checkpoint', name='before-fault')
                host_action('fault', name=FAULTS[args.scenario])
                witness = json.loads((WORK / 'p42-fault.json').read_text())
                state = ready_state(HARD_DOMAINS)
                block, outside = split_hosts(state['hosts'])
                before_block, before_outside = split_hosts(baseline['inspection']['hosts'])
                function, samples = wait_connectivity(HARD_DOMAINS)
                expected_external = external_rows(before_outside)
                if args.scenario == 'append':
                    expected_external.update([APPEND_SENTINEL])
                proof = {**function, 'fault_witnessed': witness['fault_witnessed'] and witness['immutable_clear_witnessed'],
                         'managed_exact': block == before_block, 'immutable_restored': immutable(state),
                         'policy_unchanged': rpc('config_snapshot') == baseline['policy'], 'collector_alive': collector_alive()}
                details = {'inspection': state, 'connectivity': samples,
                           'repair_elapsed_seconds': (dt.datetime.now(dt.timezone.utc) - dt.datetime.fromisoformat(witness['checked_at'])).total_seconds()}
                if args.scenario == 'delete':
                    proof.update(file_recreated=state['hosts'] is not None, external_loss_recorded=True)
                    details['external_rows_lost'] = list((external_rows(before_outside) - external_rows(outside)).elements())
                    details['external_rows_added'] = list((external_rows(outside) - external_rows(before_outside)).elements())
                else:
                    proof['unrelated_preserved'] = external_rows(outside) == expected_external
                if args.scenario == 'append':
                    details['sentinel_resolution'] = wait_hosts_mapping(
                        'unrelated.p42.blockuntu.test', '127.0.0.43', run, wait)
                    proof.update(sentinel_preserved=APPEND_SENTINEL in outside.splitlines(),
                                 sentinel_resolves=True, collector_alive=collector_alive())
                save(WORK / 'p42-repaired.json', {'proof': proof, 'details': details})
                host_action('checkpoint', name='after-fault')
                return require(proof, details)
            ready = case(cid, repaired, ready)

            if args.scenario == 'remove':
                def schedule_transitions():
                    rule_id = args.run_id + '-scheduled'
                    rpc('upsert_site_list', {'rule': {'id': rule_id, 'name': 'P42 natural Tier 2 schedule',
                        'tier': 'scheduled_block', 'enabled': True, 'schedule_ids': [],
                        'patterns': [{'kind': 'domain', 'value': SCHEDULED, 'match_subdomains': False}]}})
                    now = dt.datetime.now().astimezone()
                    # Leave at least 30 seconds for the initial measurement.
                    earliest = now + dt.timedelta(seconds=30)
                    start = earliest.replace(second=0, microsecond=0) + dt.timedelta(minutes=1)
                    end = start + dt.timedelta(minutes=1)
                    assert now.date() == start.date() == end.date(), 'Select away from local midnight'
                    rpc('upsert_schedule', {'schedule': {'id': args.run_id + '-window', 'name': 'P42 one-minute window',
                        'windows': [{'weekday': ('mon','tue','wed','thu','fri','sat','sun')[now.weekday()],
                                     'start': start.strftime('%H:%M'), 'end': end.strftime('%H:%M')}]}, 'site_rule_ids': [rule_id]})
                    stages = []
                    for name, deadline, domains, active in [('inactive', None, HARD_DOMAINS, False),
                            ('active', start, (*HARD_DOMAINS, SCHEDULED), True), ('ended', end, HARD_DOMAINS, False)]:
                        if deadline is not None:
                            wait(lambda: dt.datetime.now().astimezone() >= deadline, 90)
                        observation_started_at = dt.datetime.now().astimezone().isoformat()
                        state = ready_state(domains, 15)
                        block, outside = split_hosts(state['hosts'])
                        function, hard_samples = connectivity(HARD_DOMAINS)
                        resolution = resolution_witness(SCHEDULED, run)
                        address = resolution['native']['addresses']
                        request = http(SCHEDULED)
                        stage = {'stage': name, 'at': dt.datetime.now().astimezone().isoformat(), 'inspection': state,
                                 'observation_started_at': observation_started_at,
                                 'hard': hard_samples, 'scheduled': {'addresses': address, 'resolution': resolution, 'http': request},
                                 'hard_ok': all(function.values()),
                                 'scheduled_ok': (blocking_resolution(resolution) if active else resolution['native']['returncode'] == 0 and address == ['127.0.0.2']) and request['accessible'] == (not active),
                                 'external_ok': external_rows(outside) == external_rows(split_hosts(baseline['inspection']['hosts'])[1])}
                        stages.append(stage)
                        save(WORK / 'p42-schedule.json', {'starts_at': start.isoformat(), 'ends_at': end.isoformat(), 'stages': stages})
                        host_action('checkpoint', name='schedule-' + name)
                        assert stage['hard_ok'] and stage['scheduled_ok'] and stage['external_ok'], stage
                        schedule_observation_in_window(stage, start, end)
                    proof = {'initially_inactive': True, 'natural_activation': True, 'natural_expiry': True,
                             'scheduled_resolution_transitions': True, 'scheduled_http_transitions': True,
                             'tier1_persisted': all(s['hard_ok'] for s in stages), 'immutable_enabled': True,
                             'unrelated_preserved': all(s['external_ok'] for s in stages),
                             'clock_healthy': rpc('clock_integrity_status')['state'] == 'ok', 'collector_alive': collector_alive()}
                    return require(proof, {'starts_at': start.isoformat(), 'ends_at': end.isoformat(), 'stages': stages})
                case('DES-HOSTS-004', schedule_transitions, ready)
        else:
            def allowlist():
                rule_id = args.run_id + '-allowlist'
                rpc('upsert_site_list', {'rule': {'id': rule_id, 'name': 'P42 hosts-free allowlist',
                    'tier': 'scheduled_block', 'mode': 'allowlist', 'enabled': True, 'schedule_ids': [],
                    'patterns': [{'kind': 'domain', 'value': FREE, 'match_subdomains': True}]}})
                session = rpc('start_detox', {'name': 'P42 active allowlist', 'duration_minutes': 1, 'site_rule_ids': [rule_id]})
                denied = rpc('evaluate_url', {'url': 'http://' + OUTSIDE + ':18080/free', 'probe': True})
                allowed = rpc('evaluate_url', {'url': 'http://' + FREE + ':18080/free', 'probe': True})
                state = ready_state(())
                block, outside = split_hosts(state['hosts'])
                before_block, before_outside = split_hosts(baseline['inspection']['hosts'])
                probes = {name: {'addresses': addresses(name), 'http': http(name)} for name in (FREE, OUTSIDE)}
                active = any(s['id'] == session['session']['id'] for s in rpc('detox_sessions', {'active_only': True})['sessions'])
                proof = {'allowlist_active': active, 'outside_rpc_blocked': denied['decision'] == 'block' and denied['reason']['rule_id'] == rule_id,
                         'listed_rpc_allowed': allowed['decision'] == 'allow', 'managed_empty': not domain_rows(block),
                         'no_catch_all': block == before_block, 'immutable_enabled': immutable(state),
                         'unrelated_preserved': external_rows(outside) == external_rows(before_outside),
                         'direct_http_accessible': all(p['addresses'] == ['127.0.0.2'] and p['http']['accessible'] for p in probes.values()),
                         'collector_alive': collector_alive()}
                details = {'session': session, 'outside_decision': denied, 'listed_decision': allowed, 'inspection': state, 'connectivity': probes}
                save(WORK / 'p42-allowlist.json', {'proof': proof, 'details': details})
                host_action('checkpoint', name='allowlist-active')
                return require(proof, details)
            case('DES-HOSTS-005', allowlist, ready)
        result['status'] = 'pass' if all(row['status'] == 'pass' for row in result['cases']) else 'fail'
    except Exception as error:
        result.update(status='fail', error=redact(error), traceback=redact(traceback.format_exc()))
    checkpoint()


if __name__ == '__main__':
    main()
