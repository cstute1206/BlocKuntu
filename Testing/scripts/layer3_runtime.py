"""Translate the established smoke executor into the Layer 3 observation contract."""
from __future__ import annotations

from layer3_contract import assess_observations, build_plan

ISOLATED_UBUNTU_CASES = frozenset(('VM-WAL-001', 'VM-WAL-005', 'VM-AAL-001', 'VM-AAL-010'))
P32_WEBSITE_CASES = frozenset(('VM-WEB-003', 'VM-WEB-004', 'VM-WEB-005',
                              'VM-WEB-006', 'VM-WEB-007'))
P32_DOMAIN_CASES = frozenset(('VM-WEB-001',))
LIFECYCLE_CASES = frozenset(('VM-ACT-001', 'VM-ACT-002', 'VM-WEB-008'))
GUI_EDIT_CASES = frozenset(f'VM-EDIT-00{i}' for i in range(1, 6))
PRIVATE_CASES = frozenset(('VM-BR-004', 'VM-WAL-013'))
HELPER_CASES = frozenset(('VM-AAL-005', 'VM-AAL-019'))
DESKTOP_GROUPS = (GUI_EDIT_CASES, PRIVATE_CASES, HELPER_CASES)
P32_WEBSITE_GROUPS = (*DESKTOP_GROUPS, P32_WEBSITE_CASES, P32_DOMAIN_CASES, LIFECYCLE_CASES,
                      frozenset(('VM-WAL-012',)),
                      *(frozenset((cid,)) for cid in sorted(LIFECYCLE_CASES)))


def isolated_case(row, plan):
    return (plan['scope'] == 'selected' and plan['suite'] in ('core', 'full') and
            row['guest'] == 'ubuntu' and row['id'] in ISOLATED_UBUNTU_CASES and
            row['scenario'] == 'baseline' and row['browser_package'] is None and
            row['implementation_status'] == 'automated')


def p32_website_plan(plan):
    """Explicit desktop groups, plus the Ubuntu-only browser journeys."""
    rows = plan['observations']
    ids = {row['id'] for row in rows}
    if ids in DESKTOP_GROUPS:
        return (plan['suite'] == 'core' and plan['scope'] == 'selected' and
                len(plan['guests']) == 1 and
                plan['guests'][0] in ('ubuntu', 'fedora', 'cachyos') and all(
                    row['guest'] == plan['guests'][0] and row['implementation_status'] == 'automated' and
                    type(row['budget_seconds']) is int and row['budget_seconds'] > 0 and
                    (row['browser_package'] in ('firefox-native',
                        'chromium-native' if plan['guests'][0] == 'cachyos' else 'chrome-native')
                     if ids == PRIVATE_CASES else row['browser_package'] is None)
                    for row in rows))
    return (plan['suite'] == 'core' and plan['scope'] == 'selected' and
            plan['guests'] == ['ubuntu'] and ids in P32_WEBSITE_GROUPS and
            ((ids == {'VM-WAL-012'} and len(rows) == 1 and
              rows[0]['browser_package'] in ('firefox-native', 'chrome-native')) or
             (ids != {'VM-WAL-012'} and len(rows) == 2 * len(ids) and
              {row['browser_package'] for row in rows} == {'firefox-native', 'chrome-native'})) and
            all(row['guest'] == 'ubuntu' and row['scenario'] == 'baseline' and
                row['implementation_status'] == 'automated' and
                type(row['budget_seconds']) is int and row['budget_seconds'] > 0
                for row in rows))


def p32_website_budget_seconds(smoke_plan, selected_plan):
    if not p32_website_plan(selected_plan):
        raise ValueError('Not the executable P3.2 website group')
    return smoke_budget_seconds(smoke_plan) + sum(row['budget_seconds']
        for row in selected_plan['observations']) + selected_plan['deadlines_seconds']['desktop']


def execution_plans(registry, matrix, *, guest, suite='smoke', case_ids=None, browsers=None, scenarios=None):
    """Reject requests the present executor cannot satisfy before creating a VM.

    A selected case is a diagnostic projection of the full smoke execution.
    It never substitutes for a complete suite result.
    """
    smoke = build_plan(registry, matrix, suite='smoke', guests=[guest])
    selected = build_plan(registry, matrix, suite=suite, guests=[guest], case_ids=case_ids,
                          browsers=browsers, scenarios=scenarios)
    if (p32_website_plan(selected) and case_ids is not None and
            len(case_ids) == len(set(case_ids)) and set(case_ids) in P32_WEBSITE_GROUPS):
        return smoke, selected
    if (case_ids is not None and len(case_ids) == 1 and
            len(selected['observations']) == 1 and
            isolated_case(selected['observations'][0], selected)):
        return smoke, selected
    available = {row['observation_id'] for row in smoke['observations']}
    unsupported = [row['observation_id'] for row in selected['observations']
                   if row['observation_id'] not in available or
                   row['implementation_status'] != 'automated']
    if unsupported:
        sample = ', '.join(unsupported[:3])
        raise ValueError(f'Selected suite has {len(unsupported)} observations without an executable '
                         f'smoke binding (first: {sample}); implement their adapters before VM execution')
    return smoke, selected


def smoke_budget_seconds(plan):
    """Bound the current whole-smoke group from its executable observations."""
    observations = [row for row in plan['observations'] if row['id'] != 'VM-INSTALL-001']
    if plan['suite'] != 'smoke' or not observations or any(
            row['implementation_status'] != 'automated' or
            type(row.get('budget_seconds')) is not int for row in observations):
        raise ValueError('Smoke group lacks executable bounded observations')
    setup = plan['deadlines_seconds']['desktop']
    teardown = plan['deadlines_seconds']['gui']
    return setup + sum(row['budget_seconds'] for row in observations) + teardown


def group_budget_seconds(plan, observation_ids):
    """Bound one isolated implemented group; this does not make it executable."""
    selected = list(observation_ids)
    by_id = {row['observation_id']: row for row in plan['observations']}
    if not selected or len(selected) != len(set(selected)) or not set(selected) <= set(by_id):
        raise ValueError('Group budget requires distinct observations in the plan')
    rows = [by_id[key] for key in selected]
    if any(row['implementation_status'] != 'automated' or
           type(row.get('budget_seconds')) is not int or row['budget_seconds'] <= 0
           for row in rows):
        raise ValueError('Group budget requires implemented bounded observations')
    return (plan['deadlines_seconds']['desktop'] +
            sum(row['budget_seconds'] for row in rows) +
            plan['deadlines_seconds']['gui'])


def process_budget_seconds(plan):
    """Bound the separate root supervisor from process fixture observations."""
    observations = [row for row in plan['observations']
                    if 'root-supervisor' in row['prerequisites']]
    if plan['suite'] != 'smoke' or not observations or any(
            row['implementation_status'] != 'automated' or
            type(row.get('budget_seconds')) is not int for row in observations):
        raise ValueError('Process group lacks executable bounded observations')
    return plan['deadlines_seconds']['rpc'] + sum(row['budget_seconds'] for row in observations)


def require_stage_passed(plan, legacy_cases, stage):
    """Stop dependent smoke work when a prerequisite group did not pass."""
    selectors = {
        'services': lambda row: row['id'] == 'VM-INSTALL-002',
        'gui': lambda row: row['browser_package'] is None and
               row['id'] not in ('VM-INSTALL-001', 'VM-INSTALL-002') and
               'root-supervisor' not in row['prerequisites'],
        'browsers': lambda row: row['browser_package'] is not None,
        'processes': lambda row: 'root-supervisor' in row['prerequisites'],
    }
    if stage not in selectors or plan['suite'] != 'smoke':
        raise ValueError(f'Unknown smoke dependency stage: {stage}')
    required = [row for row in plan['observations'] if selectors[stage](row)]
    if not required:
        raise ValueError(f'Empty smoke dependency stage: {stage}')
    accepted = {(row['id'], row['browser_package'].split('-')[0]
                 if row['browser_package'] else None) for row in required}
    by_key = {}
    for row in legacy_cases:
        key = (row.get('id'), row.get('browser'))
        if key in accepted:
            by_key.setdefault(key, []).append(row.get('status'))
    bad = [key for key in sorted(accepted, key=str) if by_key.get(key) != ['pass']]
    if bad:
        raise RuntimeError(f'{stage} prerequisite observations did not pass: {bad}')


def smoke_observations(plan, legacy_cases):
    """Keep setup and unexpected rows visible while adding exact dimensions."""
    by_legacy = {}
    for expected in plan['observations']:
        package = expected['browser_package']
        key = (expected['id'], package.split('-')[0] if package else None)
        if key in by_legacy:
            raise ValueError(f'Ambiguous legacy smoke key: {key}')
        by_legacy[key] = expected
    converted, deferred = [], {row['id'] for row in plan['deferred_cases']}
    for index, row in enumerate(legacy_cases):
        if row.get('id') in deferred and row.get('status') == 'skip' and row.get('reason'):
            continue
        key = (row.get('id'), row.get('browser'))
        expected = by_legacy.get(key)
        if expected:
            converted.append({**row,
                              'observation_id': row.get('observation_id', expected['observation_id']),
                              'guest': row.get('guest', expected['guest']),
                              'browser_package': row.get('browser_package', expected['browser_package']),
                              'scenario': row.get('scenario', expected['scenario'])})
        else:
            converted.append({**row, 'observation_id': f'unexpected:{index}',
                              'guest': plan['guests'][0], 'browser_package': None,
                              'scenario': None})
    return converted


def evaluate_smoke(plan, legacy_cases):
    observations = smoke_observations(plan, legacy_cases)
    coverage = assess_observations(plan, observations)
    return observations, coverage
