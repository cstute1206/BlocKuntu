#!/usr/bin/env python3
"""Small isolated non-smoke assertions in an installed disposable guest."""
import argparse
import json
from pathlib import Path
import time

from guest import WORK, rpc, run, wait


def rejected_tier1_rule(method, rule):
    before = rpc('config_snapshot')
    assert rpc('status')['enforcement_state'] == 'active'
    try:
        rpc(method, {'rule': rule})
    except RuntimeError as error:
        rejection = str(error)
    else:
        raise AssertionError('Daemon accepted an unsupported Tier 1 allowlist')
    assert 'must use Tier 2 or Tier 3' in rejection, rejection
    assert rule['id'] in rejection, 'Rejection did not identify the attempted rule'
    assert rpc('config_snapshot') == before, 'Rejected rule changed the saved policy'
    assert rpc('status')['enforcement_state'] == 'active'
    return {'rule_id': rule['id'], 'rejection': rejection,
            'policy_unchanged': True, 'enforcement_active': True}


def tier1_website_allowlist(run_id):
    rule_id = run_id + '-tier1-allowlist'
    rule = {'id': rule_id, 'name': 'Acceptance rejected Tier 1 allowlist',
            'tier': 'hard', 'mode': 'allowlist', 'enabled': True,
            'patterns': [{'kind': 'domain', 'value': 'allowed.blockuntu.test',
                          'match_subdomains': True}], 'schedule_ids': []}
    return rejected_tier1_rule('upsert_site_list', rule)


def tier1_application_allowlist(run_id):
    rule = {'id': run_id + '-tier1-app-allowlist',
            'name': 'Acceptance rejected Tier 1 application allowlist',
            'tier': 'hard', 'mode': 'allowlist', 'enabled': True,
            'matchers': [{'kind': 'command_name', 'value': 'bk-test-app-a'}],
            'schedule_ids': []}
    return rejected_tier1_rule('upsert_app_rule', rule)


def managed_hosts_block():
    content = Path('/etc/hosts').read_text()
    begin, end = '# BEGIN BLOCKUNTU MANAGED', '# END BLOCKUNTU MANAGED'
    assert content.count(begin) == content.count(end) == 1, 'Managed hosts markers are missing or duplicated'
    return content[content.index(begin):content.index(end) + len(end)]


def active_website_allowlist_hosts(run_id):
    rule_id = run_id + '-hosts-allowlist'
    before = managed_hosts_block()
    before_status = rpc('enforcement_status')['hosts_file']
    assert before_status['managed_block_compliant'], before_status
    rule = {'id': rule_id, 'name': 'Acceptance hosts-free website allowlist',
            'tier': 'scheduled_block', 'mode': 'allowlist', 'enabled': True,
            'patterns': [{'kind': 'domain', 'value': 'allowed.blockuntu.test',
                          'match_subdomains': True}], 'schedule_ids': []}
    rpc('upsert_site_list', {'rule': rule})
    session = rpc('start_detox', {'name': 'Acceptance hosts-free website allowlist',
                                  'duration_minutes': 3, 'site_rule_ids': [rule_id]})
    outside = 'http://outside.blockuntu.test:18080/outside'
    allowed = 'http://allowed.blockuntu.test:18080/allowed'
    def outside_blocked():
        answer = rpc('evaluate_url', {'url': outside, 'probe': True})
        return answer if answer.get('decision') == 'block' else None
    rejected = wait(outside_blocked, 30)
    assert rejected['reason']['list_mode'] == 'allowlist', rejected
    assert rejected['reason']['rule_id'] == rule_id, rejected
    listed = rpc('evaluate_url', {'url': allowed, 'probe': True})
    assert listed['decision'] == 'allow', listed
    after = managed_hosts_block()
    after_status = rpc('enforcement_status')['hosts_file']
    assert after_status['managed_block_compliant'], after_status
    assert before_status['expected_domain_count'] == after_status['expected_domain_count']
    assert after == before, 'Allowlist changed the managed hosts block'
    assert 'allowed.blockuntu.test' not in after and 'outside.blockuntu.test' not in after
    assert rpc('status')['enforcement_state'] == 'active'
    return {'rule_id': rule_id, 'detox_session_id': session['session']['id'],
            'outside_rejected': rejected['decision'] == 'block',
            'listed_allowed': listed['decision'] == 'allow',
            'hosts_managed_unchanged': True, 'host_status_compliant': True,
            'managed_domain_count': after_status['expected_domain_count'],
            'enforcement_active': True}


def application_allowance_lifecycle(run_id):
    run('sudo', '-n', 'systemd-run', '--collect',
        '--unit=blockuntu-application-lifecycle', '--property=RuntimeMaxSec=480',
        '/usr/bin/python3', str(WORK / 'application_lifecycle.py'), '--run-id', run_id)
    wait((WORK / 'application-lifecycle.json').exists, 490)
    row = json.loads((WORK / 'application-lifecycle.json').read_text())
    if row.get('status') != 'pass':
        raise RuntimeError(row.get('error', 'Application lifecycle did not pass'))
    return row['evidence']


SCENARIOS = {'VM-WAL-001': tier1_website_allowlist,
             'VM-WAL-005': active_website_allowlist_hosts,
             'VM-AAL-001': tier1_application_allowlist,
             'VM-AAL-010': application_allowance_lifecycle}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--scenario', choices=sorted(SCENARIOS), required=True)
    args = parser.parse_args()
    started = time.monotonic()
    row = {'id': args.scenario, 'status': 'running'}
    try:
        row['evidence'] = SCENARIOS[args.scenario](args.run_id)
        row['status'] = 'pass'
    except Exception as error:
        row.update(status='fail', error=f'{type(error).__name__}: {error}')
    row['seconds'] = round(time.monotonic() - started, 3)
    temporary = WORK / 'scenario-result.tmp'
    temporary.write_text(json.dumps(row, indent=2) + '\n')
    temporary.replace(WORK / 'scenario-result.json')
    print(json.dumps(row))
    return int(row['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
