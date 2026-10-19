"""Read-only Layer 3 coverage planning; does not install packages or operate VMs."""
from __future__ import annotations

from collections import Counter
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SUITES = ('smoke', 'core', 'browser-matrix', 'full')


def unique(values, description):
    duplicates = [value for value, count in Counter(values).items() if count > 1]
    if duplicates:
        raise ValueError(f'Duplicate {description}: {duplicates}')


def load_contract(root=ROOT):
    root = Path(root)
    registry = json.loads((root / 'layer3/cases.json').read_text())
    matrix = json.loads((root / 'config/layer3-browsers.json').read_text())
    validate_contract(registry, matrix, (root / 'layers/03-vm-acceptance.md').read_text())
    return registry, matrix


def validate_contract(registry, matrix, specification):
    if registry['schema_version'] != 1 or matrix['schema_version'] != 1:
        raise ValueError('Unsupported Layer 3 contract schema')
    if registry['guests'] != ['ubuntu', 'fedora', 'cachyos']:
        raise ValueError('Unexpected guest scope')
    cases, variants = registry['cases'], matrix['variants']
    unique([case['id'] for case in cases], 'case IDs')
    unique([variant['id'] for variant in variants], 'browser variants')
    specified = {}
    for line in specification.splitlines():
        if re.match(r'^\| VM-[A-Z]+-\d+ \|', line):
            cid, action, expected = [part.strip() for part in line.strip('|').split('|')]
            if cid in specified:
                raise ValueError(f'Duplicate specification ID: {cid}')
            specified[cid] = (action, expected)
    if {case['id'] for case in cases} != set(specified):
        raise ValueError('Registry case IDs differ from the Layer 3 specification')
    for case in cases:
        cid = case['id']
        if (case['action'], case['expected']) != specified[cid]:
            raise ValueError(f'Specification drift: {cid}')
        if case['implementation_status'] not in ('planned', 'automated', 'manual', 'blocked'):
            raise ValueError(f'Invalid implementation status: {cid}')
        if type(case['smoke']) is not bool or type(case['deferred']) is not bool:
            raise ValueError(f'Invalid scope flags: {cid}')
        if case['browser_scope'] not in ('guest', 'supported', 'confined', 'unsupported'):
            raise ValueError(f'Invalid browser scope: {cid}')
        if not case['scenarios'] or not case['evidence'] or not case['isolation']:
            raise ValueError(f'Missing scenario/evidence/isolation contract: {cid}')
        unique([scenario['id'] for scenario in case['scenarios']], f'{cid} scenarios')
        for scenario in case['scenarios']:
            if not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', scenario['id']):
                raise ValueError(f'Invalid scenario ID: {cid}')
            families = scenario['browser_families']
            if not families or not set(families) <= {'firefox', 'chromium'}:
                raise ValueError(f'Invalid scenario family: {cid}')
            budget = scenario.get('budget_seconds')
            if budget is not None and (type(budget) is not int or not 1 <= budget <= 1800):
                raise ValueError(f'Invalid scenario budget: {cid}/{scenario["id"]}')
        if not set(case['prerequisites']) <= set(registry['prerequisites']):
            raise ValueError(f'Unknown prerequisite: {cid}')
        if case['deferred'] != (cid == 'VM-UPGRADE-001'):
            raise ValueError('Only the agreed upgrade case may be deferred')
        if case['deferred'] and (case['smoke'] or not case['deferral_reason']):
            raise ValueError('Invalid upgrade deferral')
        if case['smoke'] and (case['implementation_status'] != 'automated' or
                             'baseline' not in {s['id'] for s in case['scenarios']}):
            raise ValueError(f'Smoke implementation lacks baseline: {cid}')
    by_variant = {variant['id']: variant for variant in variants}
    for variant in variants:
        if variant['family'] not in ('firefox', 'chromium'):
            raise ValueError('Unknown browser family')
        if type(variant['expected_supported']) is not bool or type(variant['confined']) is not bool:
            raise ValueError('Invalid browser support flags')
        targets = variant['required_guests']
        unique(targets, 'browser guest cells')
        if not targets or not set(targets) <= set(registry['guests']):
            raise ValueError('Invalid browser guest cells')
    if set(matrix['core_browsers']) != set(registry['guests']):
        raise ValueError('Missing core guest browsers')
    for guest, ids in matrix['core_browsers'].items():
        unique(ids, 'core browsers')
        for vid in ids:
            variant = by_variant.get(vid)
            if not variant or not variant['expected_supported'] or guest not in variant['required_guests']:
                raise ValueError(f'Invalid core browser {guest}/{vid}')
        if {by_variant[vid]['family'] for vid in ids} != {'firefox', 'chromium'}:
            raise ValueError(f'Core guest lacks both browser families: {guest}')
    expected_cells = {f"{guest}/{variant['id']}" for variant in variants
                      for guest in variant['required_guests']}
    cells = matrix.get('cells', {})
    if set(cells) != expected_cells:
        raise ValueError('Browser cell inventory differs from the required matrix')
    details = ('source', 'observed_version', 'version_evidence', 'install_method',
               'executable', 'profile_root', 'policy_paths', 'native_messaging_paths')
    for key, cell in cells.items():
        guest, variant_id = key.split('/', 1)
        if cell.get('status') == 'planned':
            if any(cell.get(field) is not None for field in details) or not cell.get('pending_reason'):
                raise ValueError(f'Planned browser cell has unverified layout data: {key}')
        elif cell.get('status') == 'smoke-baseline':
            if variant_id not in matrix['core_browsers'][guest] or cell.get('pending_reason') is not None:
                raise ValueError(f'Invalid smoke browser cell: {key}')
            if any(not isinstance(cell.get(field), str) or not cell[field] for field in details[:6]):
                raise ValueError(f'Incomplete smoke browser cell: {key}')
            if not cell['version_evidence'].startswith('results/p31-') or '..' in cell['version_evidence']:
                raise ValueError(f'Invalid browser version evidence: {key}')
            if not cell['profile_root'].startswith('/home/akhi/Testing/layer3/'):
                raise ValueError(f'Invalid dedicated browser profile: {key}')
            for field in ('policy_paths', 'native_messaging_paths'):
                paths = cell.get(field)
                if not isinstance(paths, list) or not paths or any(
                        not isinstance(path, str) or not path.startswith('/') or '..' in path
                        for path in paths):
                    raise ValueError(f'Invalid {field} for browser cell: {key}')
                unique(paths, f'{key} {field}')
        else:
            raise ValueError(f'Invalid browser cell status: {key}')
    bindings = registry['implementation_bindings']
    unique([observation_key(binding) for binding in bindings], 'implementation bindings')
    by_case = {case['id']: case for case in cases}
    for binding in bindings:
        case = by_case.get(binding['id'])
        guest, browser = binding['guest'], binding['browser_package']
        if not case or case['deferred'] or guest not in registry['guests']:
            raise ValueError('Invalid implementation binding case/guest')
        scenarios = {s['id']: s for s in case['scenarios']}
        scenario = scenarios.get(binding['scenario'])
        if not scenario or binding['status'] not in ('automated', 'manual') or not binding['entrypoint']:
            raise ValueError('Invalid implementation binding scenario/status/entrypoint')
        if scenario.get('budget_seconds') is None:
            raise ValueError('Implemented observation requires a bounded scenario budget')
        if case['browser_scope'] == 'guest':
            if browser is not None:
                raise ValueError('Guest-only binding has a browser')
        else:
            variant = by_variant.get(browser)
            if not variant or guest not in variant['required_guests'] or variant['family'] not in scenario['browser_families']:
                raise ValueError('Implementation binding has an invalid browser dimension')
            if variant['expected_supported'] != (case['browser_scope'] != 'unsupported'):
                raise ValueError('Implementation binding has the wrong support expectation')
            if case['browser_scope'] == 'confined' and not variant['confined']:
                raise ValueError('Confined implementation binding requires a confined package')


def observation_key(row):
    return (row['id'], row['guest'], row['browser_package'], row['scenario'])


def build_plan(registry, matrix, *, suite='smoke', guests=None, case_ids=None, browsers=None, scenarios=None):
    if suite not in SUITES:
        raise ValueError(f'Unknown suite: {suite}')
    guests = registry['guests'] if guests is None else list(guests)
    if not guests or not set(guests) <= set(registry['guests']):
        raise ValueError('Unknown or empty guest selection')
    unique(guests, 'selected guests')
    if case_ids is not None:
        case_ids = list(case_ids)
        unique(case_ids, 'selected cases')
        if not case_ids or not set(case_ids) <= {case['id'] for case in registry['cases']}:
            raise ValueError('Unknown or empty case selection')
    if browsers is not None:
        browsers = list(browsers)
        unique(browsers, 'selected browsers')
        if not browsers or not set(browsers) <= {v['id'] for v in matrix['variants']}:
            raise ValueError('Unknown or empty browser selection')
    if scenarios is not None:
        scenarios = list(scenarios)
        unique(scenarios, 'selected scenarios')
        if not scenarios or case_ids is None:
            raise ValueError('Scenario selection requires explicit cases and nonempty scenarios')
    bindings = {observation_key(binding): binding for binding in registry['implementation_bindings']}
    observations, deferred = [], []
    for case in registry['cases']:
        if case_ids is not None and case['id'] not in case_ids:
            continue
        if case['deferred']:
            deferred.append({'id': case['id'], 'reason': case['deferral_reason']})
            continue
        if suite == 'smoke' and not case['smoke']:
            continue
        scope = case['browser_scope']
        if suite == 'browser-matrix' and scope == 'guest':
            continue
        for guest in guests:
            variants = [None] if scope == 'guest' else [
                v for v in matrix['variants'] if guest in v['required_guests'] and
                (v['expected_supported'] if scope != 'unsupported' else not v['expected_supported']) and
                (scope != 'confined' or v['confined']) and
                (suite not in ('smoke', 'core') or v['id'] in matrix['core_browsers'][guest]) and
                (browsers is None or v['id'] in browsers)]
            for variant in variants:
                for scenario in case['scenarios']:
                    if scenarios is not None and scenario['id'] not in scenarios:
                        continue
                    if suite == 'smoke' and scenario['id'] != 'baseline':
                        continue
                    if variant and variant['family'] not in scenario['browser_families']:
                        continue
                    browser = variant['id'] if variant else None
                    binding = bindings.get((case['id'], guest, browser, scenario['id']))
                    observations.append({
                        'observation_id': ':'.join([guest, case['id'], browser or 'none', scenario['id']]),
                        'id': case['id'], 'guest': guest, 'browser_package': browser,
                        'scenario': scenario['id'],
                        'implementation_status': binding['status'] if binding else 'planned',
                        'execution_status': 'not_run', 'prerequisites': case['prerequisites'],
                        'isolation': case['isolation'], 'required_evidence': case['evidence'],
                        'budget_seconds': scenario.get('budget_seconds'),
                    })
    unique([row['observation_id'] for row in observations], 'planned observations')
    selected_cases = {row['id'] for row in observations}
    if scenarios is not None and set(scenarios) != {row['scenario'] for row in observations}:
        raise ValueError('Selected scenario is not applicable to this case/browser selection')
    if case_ids is not None and set(case_ids) != selected_cases:
        raise ValueError('Selected case is deferred or not applicable to this suite/selection')
    selected_browsers = {row['browser_package'] for row in observations if row['browser_package']}
    if browsers is not None and set(browsers) != selected_browsers:
        raise ValueError('Selected browser is not applicable to this suite/selection')
    if not observations:
        raise ValueError('Selection has no required observations')
    return {
        'schema_version': 1, 'kind': 'coverage-plan', 'suite': suite,
        'scope': 'selected' if case_ids is not None or browsers is not None else 'suite',
        'guests': guests, 'acceptance_status': 'not_evaluated',
        'required_count': len(observations), 'case_count': len(selected_cases),
        'implementation_counts': dict(Counter(row['implementation_status'] for row in observations)),
        'deadlines_seconds': registry['deadlines_seconds'],
        'observation_windows_seconds': registry['observation_windows_seconds'],
        'scenario_durations_seconds': registry['scenario_durations_seconds'],
        'deferred_cases': deferred, 'observations': observations,
    }


def assess_observations(plan, results):
    """Check exact observation coverage, separately from VM/provenance acceptance.

    This cannot convert legacy smoke reports to full acceptance or assert that
    a package/environment is trusted. The VM runner will supply those gates.
    """
    required = {row['observation_id']: row for row in plan['observations']}
    unique([row['observation_id'] for row in plan['observations']], 'plan observations')
    observed, errors = {}, []
    for row in results:
        key = row.get('observation_id')
        if key not in required:
            errors.append('Unknown or missing observation ID: ' + str(key))
            continue
        if key in observed:
            errors.append('Duplicate observation: ' + key)
            continue
        expected = required[key]
        if any(row.get(field) != expected[field] for field in ('id', 'guest', 'browser_package', 'scenario')):
            errors.append('Observation dimensions differ: ' + key)
        status = row.get('status')
        if status not in ('pass', 'fail', 'blocked', 'skip', 'not_run'):
            errors.append('Unknown execution status: ' + key)
        if status == 'pass' and expected['implementation_status'] not in ('automated', 'manual'):
            errors.append('Unimplemented observation claims a pass: ' + key)
        observed[key] = row
    missing = sorted(key for key in required if observed.get(key, {}).get('status') != 'pass')
    failed = [key for key, row in observed.items() if row.get('status') == 'fail']
    return {
        'observation_status': 'fail' if errors or failed else
                              'incomplete' if missing or not required else 'pass',
        'acceptance_status': 'not_evaluated', 'scope': plan['scope'], 'suite': plan['suite'],
        'missing_required_observations': missing, 'errors': errors,
        'failed_observations': failed,
    }
