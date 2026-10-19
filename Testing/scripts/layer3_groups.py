"""Conservative fresh-clone boundaries for Layer 3 scenario planning.

The whole-smoke group and selected Ubuntu scenarios have executors. Other
observations get separate clone boundaries until tested executors exist.
"""
from __future__ import annotations

from layer3_contract import build_plan, load_contract
from layer3_runtime import (P32_DOMAIN_CASES, P32_WEBSITE_CASES, group_budget_seconds, isolated_case, p32_website_budget_seconds,
                            p32_website_plan, smoke_budget_seconds)


def build_execution_groups(plan):
    if not plan['observations']:
        raise ValueError('Cannot group an empty coverage plan')
    if plan['suite'] == 'smoke' and plan['scope'] == 'suite':
        groups = []
        for guest in plan['guests']:
            rows = [row for row in plan['observations'] if row['guest'] == guest]
            if len(rows) != 21 or any(row['isolation'] != 'fresh-clone-or-verified-clean-checkpoint'
                                      for row in rows):
                raise ValueError(f'Whole-smoke clone contract changed for {guest}')
            subset = {**plan, 'guests': [guest], 'observations': rows}
            groups.append({'group_id': f'{guest}:smoke-baseline', 'guest': guest,
                           'clone_policy': 'fresh-copy', 'executor': 'vm-acceptance.py',
                           'runnable': True, 'budget_seconds': smoke_budget_seconds(subset),
                           'prerequisites': sorted({p for row in rows for p in row['prerequisites']}),
                           'observation_ids': [row['observation_id'] for row in rows],
                           'teardown': 'stop-verify-base-and-guarded-cleanup'})
    elif p32_website_plan(plan):
        registry, matrix = load_contract()
        guest = plan['guests'][0]
        smoke = build_plan(registry, matrix, suite='smoke', guests=[guest])
        domain = {row['id'] for row in plan['observations']} == P32_DOMAIN_CASES
        groups = [{'group_id': (guest+':p32-native-domain' if domain else
                                guest+':p32-native-website-matchers' if {row['id'] for row in plan['observations']} == P32_WEBSITE_CASES else
                                guest+':targeted-' + '-'.join(sorted({row['id'].lower() for row in plan['observations']}))), 'guest': guest,
                   'clone_policy': 'fresh-copy', 'executor': 'vm-acceptance.py',
                   'runnable': True, 'budget_seconds': p32_website_budget_seconds(smoke, plan),
                   'prerequisites': sorted({p for row in plan['observations'] for p in row['prerequisites']}),
                   'observation_ids': [row['observation_id'] for row in plan['observations']],
                   'teardown': 'stop-verify-base-and-guarded-cleanup'}]
    else:
        groups = [{'group_id': row['observation_id'], 'guest': row['guest'],
                   'clone_policy': 'fresh-copy',
                   'executor': 'vm-acceptance.py' if isolated_case(row, plan) else None,
                   'runnable': isolated_case(row, plan),
                   'budget_seconds': (group_budget_seconds(plan, [row['observation_id']])
                                      if row['implementation_status'] == 'automated' else None),
                   'prerequisites': row['prerequisites'],
                   'observation_ids': [row['observation_id']],
                   'teardown': 'stop-verify-base-and-guarded-cleanup'}
                  for row in plan['observations']]
    observed = [oid for group in groups for oid in group['observation_ids']]
    if len(observed) != len(set(observed)) or set(observed) != {
            row['observation_id'] for row in plan['observations']}:
        raise ValueError('Execution groups lost or duplicated required observations')
    return {'kind': 'execution-group-plan', 'suite': plan['suite'], 'scope': plan['scope'],
            'guests': plan['guests'], 'group_count': len(groups),
            'runnable_group_count': sum(group['runnable'] for group in groups),
            'required_observation_count': len(observed), 'groups': groups,
            'acceptance_status': 'not_evaluated'}
