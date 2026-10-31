"""Serialized execution of explicitly runnable Layer 3 fresh-clone groups."""
from __future__ import annotations

import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import sys

from check_vm_smoke import check_run
from layer3_contract import ROOT, assess_observations, build_plan, load_contract
from layer3_groups import build_execution_groups
from layer3_runtime import (ISOLATED_UBUNTU_CASES, P32_WEBSITE_GROUPS,
                            GUI_EDIT_CASES, PRIVATE_CASES, HELPER_CASES, p32_website_plan)

EXECUTOR = ROOT / 'scripts/vm-acceptance.py'


def read_json(path):
    return json.loads(Path(path).read_text())


def save_summary(path, summary):
    """Replace only this run's private summary, never an acceptance bundle."""
    temporary = path.with_name(path.name + '.tmp')
    with temporary.open('w') as output:
        json.dump(summary, output, indent=2)
        output.write('\n')
    os.replace(temporary, path)


def acceptance_module():
    spec = importlib.util.spec_from_file_location('layer3_vm_acceptance', EXECUTOR)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def preflight(suite, guests, prefix, verified_inputs, *, case_ids=None, browsers=None, scenarios=None, root=ROOT,
              verifier=None):
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,39}', prefix):
        raise ValueError('Run prefix requires 1-40 lowercase letters, digits or hyphens')
    registry, matrix = load_contract(root)
    plan = build_plan(registry, matrix, suite=suite, guests=guests, case_ids=case_ids,
                      browsers=browsers, scenarios=scenarios)
    grouped = build_execution_groups(plan)
    if any(not group['runnable'] or group['executor'] != 'vm-acceptance.py'
           for group in grouped['groups']):
        raise ValueError('Selection contains groups without an implemented executor; no VM started')
    isolated = (suite in ('core', 'full') and case_ids is not None and
                plan['guests'] == ['ubuntu'] and len(plan['observations']) == len(case_ids) and
                set(case_ids) <= ISOLATED_UBUNTU_CASES)
    p32 = (p32_website_plan(plan) and case_ids is not None and
           len(case_ids) == len(set(case_ids)) and set(case_ids) in P32_WEBSITE_GROUPS)
    if not isolated and not p32 and (grouped['suite'] != 'smoke' or grouped['scope'] != 'suite'):
        raise ValueError('Only complete smoke or explicitly bound guest groups have executors')
    inputs = read_json(verified_inputs)
    workflow = inputs.get('workflow', {})
    commit = workflow.get('headSha')
    if (workflow.get('status') != 'completed' or workflow.get('conclusion') != 'success' or
            not isinstance(commit, str) or not re.fullmatch(r'[0-9a-f]{40}', commit)):
        raise ValueError('Verified workflow must be successful with a full source commit')
    packages = inputs.get('packages', [])
    if not isinstance(packages, list) or not all(isinstance(item, dict) for item in packages):
        raise ValueError('Verified inputs require package records')
    if len(packages) != len({item.get('guest') for item in packages}):
        raise ValueError('Duplicate package guest in verified inputs')
    selected = {item.get('guest'): item for item in packages}
    if not set(plan['guests']) <= set(selected):
        raise ValueError('Verified inputs are missing a selected guest package')
    verifier = verifier or acceptance_module().artifact
    expected = {}
    for guest in plan['guests']:
        item = selected[guest]
        if item.get('build_provenance_verified') is not True:
            raise ValueError(f'Package provenance is unverified for {guest}')
        report_path = Path(item['report']).resolve()
        package, report = verifier(report_path, guest)
        package = Path(package).resolve()
        if (package != Path(item['package']).resolve() or
                package.name != Path(item['package']).name or
                report.get('commit') != commit or
                report.get('package_metadata', {}).get('version') != item.get('version') or
                not re.fullmatch(r'[0-9a-f]{64}', item.get('sha256', '')) or
                not any(a.get('filename') == package.name and a.get('sha256') == item['sha256']
                        for a in report.get('artifacts', []))):
            raise ValueError(f'Verified input differs from the exact {guest} package report')
        expected[guest] = {'guest': guest, 'report': str(report_path),
                           'filename': package.name, 'sha256': item['sha256'],
                           'version': item['version'], 'commit': commit}
    runs = []
    for index, group in enumerate(grouped['groups'], 1):
        guest = group['guest']
        case_id = (group['observation_ids'][0].split(':')[1] if isolated else None)
        run_id = f'{prefix}-{guest}' + (f'-{index:02d}' if isolated and len(grouped['groups']) > 1 else '')
        if len(run_id) > 40 or not re.fullmatch(r'[a-z0-9][a-z0-9-]*', run_id):
            raise ValueError(f'Generated run ID is invalid: {run_id}')
        result_dir = root / 'results' / run_id
        owner_dir = root / 'runtime/vm-phase0' / run_id
        if result_dir.exists() or owner_dir.exists():
            raise FileExistsError(f'Run ID already owns evidence or a clone: {run_id}')
        runs.append({'group_id': group['group_id'], 'guest': guest, 'run_id': run_id,
                     'case_id': case_id,
                     'case_ids': case_ids if p32 else ([case_id] if case_id else []),
                     'observation_ids': group['observation_ids'],
                     'budget_seconds': group['budget_seconds'], 'status': 'not_run',
                     'result_path': str(result_dir / guest / 'acceptance/result.json')})
    if len({item['run_id'] for item in runs}) != len(runs):
        raise ValueError('Multiple groups would reuse a run ID; add explicit unique group IDs before execution')
    output_dir = root / 'results' / f'{prefix}-group-run'
    if output_dir.exists():
        raise FileExistsError(f'Group-run evidence already exists: {output_dir}')
    return {'plan': plan, 'groups': grouped, 'expected': expected, 'runs': runs,
            'case_ids': case_ids, 'scenarios': scenarios, 'browsers': browsers,
            'output_dir': output_dir, 'commit': commit, 'workflow': workflow}


def verify_bundle(item, expected, plan, *, root=ROOT):
    run_id, guest = item['run_id'], item['guest']
    base = root / 'results' / run_id / guest
    acceptance = base / 'acceptance'
    result = read_json(acceptance / 'result.json')
    if result.get('run_id') != run_id or result.get('guest') != guest:
        raise ValueError('Acceptance result has the wrong run ID or guest')
    if result.get('status') != 'pass':
        failure = result.get('primary_failure') or next(
            (row for row in result.get('cases', []) if row.get('status') == 'fail'), None)
        detail = (failure['id'] + ': ' + failure.get('error', 'Observation failed') if failure
                  else result.get('error') or 'Executor did not complete successfully')
        return {'valid': False, 'errors': [detail], 'primary_failure': failure,
                'required_passes': 0}
    required_rows = [row for row in plan['observations']
                     if row['guest'] == guest and row['observation_id'] in item['observation_ids']]
    if set(item['observation_ids']) != {row['observation_id'] for row in required_rows}:
        raise ValueError('Group observations differ from the selected plan')
    if plan['scope'] == 'selected' and len(required_rows) == 1 and required_rows[0]['id'] in ISOLATED_UBUNTU_CASES:
        subset = {**plan, 'observations': required_rows, 'required_count': 1, 'case_count': 1}
        return verify_isolated_bundle(item, expected, subset, result, root=root)
    if p32_website_plan(plan):
        return verify_p32_bundle(item, expected, plan, result, root=root)
    required = {(row['id'], row['browser_package'].removesuffix('-native')
                 if row['browser_package'] else None) for row in required_rows}
    checked = check_run(result, read_json(acceptance / 'package-report.json'),
        read_json(acceptance / 'harness-files.json'),
        read_json(acceptance / 'browser-metadata.json'),
        read_json(root / 'runtime/vm-phase0' / run_id / guest / 'owner.json'),
        read_json(base / 'result.json'), expected, required)
    if result.get('smoke_coverage', {}).get('observation_status') != 'pass':
        checked['errors'].append('Dimensioned smoke coverage did not pass')
    observations = result.get('observations', [])
    if (len(observations) != len(item['observation_ids']) or
            {row.get('observation_id') for row in observations} != set(item['observation_ids']) or
            any(row.get('status') != 'pass' for row in observations)):
        checked['errors'].append('Dimensioned observations are missing or did not pass')
    checked['valid'] = not checked['errors']
    return checked


def verify_isolated_bundle(item, expected, plan, result, *, root=ROOT):
    """Verify the whole fresh-clone envelope as well as the selected assertion."""
    run_id, guest = item['run_id'], item['guest']
    base = root / 'results' / run_id / guest
    acceptance = base / 'acceptance'
    package = read_json(acceptance / 'package-report.json')
    manifest = read_json(acceptance / 'harness-files.json')
    owner = read_json(root / 'runtime/vm-phase0' / run_id / guest / 'owner.json')
    phase0 = read_json(base / 'result.json')
    errors = []
    case_id = plan['observations'][0]['id']
    if (result.get('suite') != f'{guest}-{plan["suite"]}-selected' or
            result.get('selected_case_ids') != [case_id] or
            result.get('coverage_scope') != 'selected' or result.get('status') != 'pass' or
            result.get('base_integrity_verified') is not True or
            result.get('error') or result.get('cleanup_error')):
        errors.append('Selected scenario result or base-integrity gate failed')
    if (package.get('passed') is not True or package.get('build_provenance_verified') is not True or
            result.get('package_commit') != expected['commit'] or package.get('commit') != expected['commit'] or
            result.get('package') != package.get('artifacts') or
            package.get('package_metadata', {}).get('version') != expected['version'] or
            not any(a.get('filename') == expected['filename'] and a.get('sha256') == expected['sha256']
                    for a in package.get('artifacts', []))):
        errors.append('Exact package provenance differs')
    if (owner.get('run_id') != run_id or owner.get('template') != guest or
            owner.get('stage') != 'removed' or not owner.get('uuid') or not owner.get('bases') or
            phase0.get('status') != 'passed' or phase0.get('clone_uuid') != owner.get('uuid') or
            not phase0.get('machine_id') or not phase0.get('host_fingerprint')):
        errors.append('Fresh clone identity/removal evidence is incomplete')
    essential = {'scripts/vm-acceptance.py', 'scripts/phase0-vm.py',
                 'fixtures/layer3/scenarios.py', 'fixtures/layer3/guest.py'}
    if case_id == 'VM-AAL-010':
        essential |= {'fixtures/layer3/application_lifecycle.py', 'fixtures/layer3/lifecycle.py'}
    if (not essential <= set(manifest) or
            not all(re.fullmatch(r'[0-9a-f]{64}', h) for h in manifest.values()) or
            result.get('harness_sha256') != manifest.get('scripts/vm-acceptance.py')):
        errors.append('Harness manifest is incomplete')
    observations = result.get('observations', [])
    coverage = assess_observations(plan, observations)
    if (coverage['observation_status'] != 'pass' or result.get('selected_coverage') != coverage or
            len(observations) != 1 or not isolated_assertion_valid(case_id,
                observations[0].get('evidence', {}) if observations else {})):
        errors.append('Selected observation or assertion evidence failed')
    if (len(result.get('cases', [])) < 2 or
            not any(row.get('id') == 'VM-INSTALL-001' and row.get('version') == expected['version']
                    and row.get('status') == 'pass' for row in result['cases'])):
        errors.append('Installed package version case is missing')
    actual = [row for row in result.get('cases', []) if row.get('status') != 'planned']
    if (len(actual) != 2 or [row.get('id') for row in actual] !=
            ['VM-INSTALL-001', case_id] or actual[1] !=
            {key: observations[0][key] for key in actual[1]}):
        errors.append('Case rows differ from the isolated observation')
    try:
        start, finish = (dt.datetime.fromisoformat(result[key]) for key in ('started_at', 'finished_at'))
        if start.tzinfo is None or finish.tzinfo is None or finish <= start:
            raise ValueError('Invalid time interval')
    except (KeyError, TypeError, ValueError):
        errors.append('Run timestamps are invalid')
    scenario = read_json(acceptance / 'scenario-result.json')
    if scenario != {k: observations[0][k] for k in scenario}:
        errors.append('Guest scenario checkpoint differs from host observation')
    if case_id == 'VM-AAL-010':
        checkpoint = read_json(acceptance / 'application-lifecycle.json')
        if (checkpoint.get('status') != 'pass' or not observations or
                checkpoint.get('evidence') != observations[0].get('evidence') or
                not 0 < scenario.get('seconds', 0) <= plan['observations'][0]['budget_seconds']):
            errors.append('Process lifecycle checkpoint or budget failed')
    return {'valid': not errors, 'errors': errors, 'required_passes': int(not errors),
            'run_id': run_id, 'guest': guest, 'package_sha256': expected['sha256']}


def isolated_assertion_valid(case_id, evidence):
    if not isinstance(evidence, dict) or evidence.get('enforcement_active') is not True:
        return False
    if case_id in ('VM-WAL-001', 'VM-AAL-001'):
        return (evidence.get('policy_unchanged') is True and
                'must use Tier 2 or Tier 3' in evidence.get('rejection', '') and
                isinstance(evidence.get('rule_id'), str) and
                evidence['rule_id'] in evidence['rejection'])
    if case_id == 'VM-AAL-010':
        identities = [evidence.get(key, {}) for key in
                      ('allowed_process', 'rejected_process', 'unlocked_process')]
        if not all(type(row.get('pid')) is int and row['pid'] > 0 and
                   isinstance(row.get('start_ticks'), str) and row['start_ticks'].isdigit() and
                   str(row.get('exe', '')).endswith('/bk-test-app-' + suffix)
                   for row, suffix in zip(identities, ('a', 'b', 'b'))):
            return False
        if (identities[1]['pid'], identities[1]['start_ticks']) == (identities[2]['pid'], identities[2]['start_ticks']):
            return False
        return (all(evidence.get(key) is True for key in
                    ('allowed_only_uncharged', 'positive_usage', 'unlocked_process_survived',
                     'reblocked_after_expiry')) and evidence.get('grant_seconds') == 120 and
                evidence.get('remaining_after_exhaustion') == 0 and
                0 < evidence.get('remaining_after_positive_usage', 0) < 60 and
                0 < evidence.get('exhaustion_seconds', 0) <= 120 and
                0 <= evidence.get('reblock_seconds', -1) <= 30 and
                evidence.get('grant', {}).get('rule_id') == evidence.get('rule_id'))
    if case_id == 'VM-WAL-005':
        return all(evidence.get(key) is True for key in
                   ('outside_rejected', 'listed_allowed', 'hosts_managed_unchanged',
                    'host_status_compliant')) and isinstance(evidence.get('managed_domain_count'), int)
    return False


def lifecycle_assertion_valid(case_id, evidence):
    """Reject partial lifecycle evidence even when a guest row says pass."""
    if not isinstance(evidence, dict):
        return False
    decision = evidence.get('blocked_decision', {})
    if (decision.get('decision') != 'block' or
            decision.get('reason', {}).get('rule_id') != evidence.get('rule_id')):
        return False
    if case_id in ('VM-WEB-008', 'VM-WAL-012'):
        used = evidence.get('remaining_after_positive_usage')
        elapsed = evidence.get('exhaustion_seconds')
        return ((case_id != 'VM-WAL-012' or
                 (evidence.get('grant_seconds') == 120 and
                  evidence.get('grant', {}).get('rule_id') == evidence.get('rule_id') and
                  all(evidence.get(key) is True for key in
                      ('allowed_only_uncharged', 'unlocked_access', 'reblocked_after_expiry')))) and
                type(used) is int and 0 < used < 60 and
                evidence.get('remaining_after_exhaustion') == 0 and
                isinstance(elapsed, (int, float)) and 0 < elapsed <= 120 and
                decision['reason'].get('controlled_reason') == 'allowance_exhausted')
    if case_id in ('VM-ACT-001', 'VM-ACT-002'):
        before, after = evidence.get('before_process', {}), evidence.get('after_process', {})
        elapsed = evidence.get('process_exit_seconds')
        return (all(type(row.get('pid')) is int and row['pid'] > 0 and
                    isinstance(row.get('start_ticks'), str) and row['start_ticks'].isdigit() and
                    str(row.get('exe', '')).endswith('/bk-test-app-a') for row in (before, after)) and
                (before['pid'], before['start_ticks']) != (after['pid'], after['start_ticks']) and
                isinstance(elapsed, (int, float)) and 0 <= elapsed <= 30 and
                evidence.get('activation') == ('schedule' if case_id == 'VM-ACT-001' else 'detox'))
    return False


def desktop_assertion_valid(case_id, scenario, evidence):
    if not isinstance(evidence, dict) or not isinstance(evidence.get('rule_id'), str):
        return False
    def identity(row, suffix):
        return (isinstance(row, dict) and type(row.get('pid')) is int and row['pid']>0 and
                str(row.get('exe','')).endswith('/'+suffix) and
                isinstance(row.get('start_ticks'),str) and row['start_ticks'].isdigit())
    if case_id in GUI_EDIT_CASES:
        if case_id in ('VM-EDIT-001','VM-EDIT-003'):
            if not all(evidence.get(k) is True for k in ('gui_created','gui_save_persisted','stricter_enforced','target_was_accessible')):
                return False
            if case_id=='VM-EDIT-003' and evidence.get('warning_confirmed') is not True:
                return False
            if scenario=='website':return evidence.get('visible_named_block') is True
            if case_id=='VM-EDIT-003' and evidence.get('session_entries_preserved') is not True:
                return False
            return (scenario=='application' and evidence.get('process_terminated') is True and
                    0<=evidence.get('termination_seconds',-1)<=30 and
                    identity(evidence.get('process_identity'),'bk-test-app-b'))
        return (all(evidence.get(k) is True for k in ('gui_control_disabled','daemon_rejected','policy_unchanged')) and
                'active' in evidence.get('rejection','') and 'cannot be edited' in evidence['rejection'])
    if case_id in HELPER_CASES:
        parent,helper=evidence.get('parent_identity',{}),evidence.get('helper_identity',{})
        uid,other_uid=evidence.get('desktop_user_uid'),evidence.get('second_account_uid')
        other_parent=evidence.get('second_user_parent_identity',{})
        other_helper=evidence.get('second_user_helper_identity',{})
        other_restored=evidence.get('second_user_restored_helper_identity',{})
        return (all(evidence.get(k) is True for k in ('helper_terminated','parent_survived','native_survived',
                    'gui_survived','protected_paths_omitted','restored_helper_survived',
                    'gui_health_available_during_restriction','rpc_available_during_restriction',
                    'second_user_enforced','second_user_control_survived','second_user_restored_helper_survived')) and
                type(uid) is int and type(other_uid) is int and 1000<=uid<65534 and 1000<=other_uid<65534 and uid!=other_uid and
                identity(other_parent,'bk-test-parent') and identity(other_helper,'bk-test-app-b') and
                identity(other_restored,'bk-test-app-b') and all(row.get('uid')==other_uid for row in (other_parent,other_helper,other_restored)) and
                other_helper.get('exe')==helper.get('exe') and other_parent.get('exe')==parent.get('exe') and
                other_restored.get('pid')!=other_helper.get('pid') and
                0<=evidence.get('second_user_termination_seconds',-1)<=30 and
                identity(parent,'bk-test-parent') and identity(helper,'bk-test-app-b') and
                helper.get('parent_pid')==parent['pid'] and helper['pid']!=parent['pid'] and
                identity(evidence.get('native_identity'),'blockuntu-native') and
                identity(evidence.get('gui_identity'),'blockuntu-gui') and
                identity(evidence.get('restored_helper_identity'),'bk-test-app-b') and
                evidence['restored_helper_identity']['pid']!=helper['pid'] and
                0<=evidence.get('termination_seconds',-1)<=30)
    if case_id in PRIVATE_CASES:
        if scenario=='regular':return evidence.get('regular_blocked') is True and evidence.get('allowed_access') is True
        if scenario in ('private','incognito-extension-permitted'):
            return (all(evidence.get(k) is True for k in ('private_window_opened','private_start_page_observed','private_document_verified','private_blocked','allowed_access')) and
                    evidence.get('policy_private_extension_permitted' if scenario=='private' else 'explicit_extension_consent') is True)
        if scenario=='incognito-configured-unprotected':
            return (evidence.get('configured_mode')=='manual_consent' and evidence.get('explicit_extension_consent') is False and
                    all(evidence.get(k) is True for k in ('private_window_opened','private_start_page_observed','private_document_verified',
                        'private_unprotected_as_configured','gui_settings_locked','daemon_settings_rejected','settings_unchanged')))
        if scenario=='incognito-disabled':
            return (evidence.get('configured_mode')=='disabled' and evidence.get('policy_incognito_mode')==1 and
                    all(evidence.get(k) is True for k in ('private_creation_prevented','regular_blocked',
                        'gui_settings_locked','daemon_settings_rejected','settings_unchanged')))
    return False


def verify_p32_bundle(item, expected, plan, result, *, root=ROOT):
    """Keep the smoke prerequisite and new browser observations separate."""
    run_id, guest = item['run_id'], item['guest']
    base = root / 'results' / run_id / guest
    acceptance = base / 'acceptance'
    registry, matrix = load_contract(root)
    smoke = build_plan(registry, matrix, suite='smoke', guests=[guest])
    required = {(row['id'], row['browser_package'].removesuffix('-native')
                 if row['browser_package'] else None) for row in smoke['observations']}
    projected = {**result, 'suite': f'{guest}-smoke'}
    checked = check_run(projected, read_json(acceptance / 'package-report.json'),
        read_json(acceptance / 'harness-files.json'),
        read_json(acceptance / 'browser-metadata.json'),
        read_json(root / 'runtime/vm-phase0' / run_id / guest / 'owner.json'),
        read_json(base / 'result.json'), expected, required)
    errors = checked['errors']
    selected_ids = {row['id'] for row in plan['observations']}
    if (result.get('suite') != f'{guest}-core-selected' or
            result.get('coverage_scope') != 'selected' or
            len(result.get('selected_case_ids') or []) != len(selected_ids) or
            set(result['selected_case_ids']) != selected_ids or
            result.get('smoke_coverage', {}).get('observation_status') != 'pass' or
            len(result.get('smoke_observations', [])) != len(smoke['observations']) or
            result.get('status') != 'pass'):
        errors.append('Website group or smoke prerequisite did not pass')
    rows = read_json(acceptance / 'p32-result.json')
    if rows != result.get('p32_cases') or len(rows) != len(plan['observations']):
        errors.append('Website checkpoint differs from host cases')
    observations = result.get('observations', [])
    coverage = assess_observations(plan, observations)
    if coverage['observation_status'] != 'pass' or result.get('selected_coverage') != coverage:
        errors.append('Dimensioned website observations did not pass')
    if len(observations) == len(plan['observations']):
        by_id = {row['observation_id']: row for row in observations}
        for expected_row in plan['observations']:
            row = by_id.get(expected_row['observation_id'], {})
            evidence = row.get('evidence', {})
            cid = expected_row['id']
            if cid in GUI_EDIT_CASES | PRIVATE_CASES | HELPER_CASES:
                if (not desktop_assertion_valid(cid, expected_row['scenario'], evidence) or
                        not isinstance(row.get('seconds'), (int, float)) or
                        not 0<=row['seconds']<=expected_row['budget_seconds']):
                    errors.append(f'Invalid desktop assertion evidence: {expected_row["observation_id"]}')
                screenshot = ('gui-'+cid.lower()+'-'+expected_row['scenario'] if cid in GUI_EDIT_CASES else
                              'gui-helper-recovery-health' if cid in HELPER_CASES else
                              expected_row['browser_package'].removesuffix('-native')+'-'+cid.lower()+'-'+expected_row['scenario'])
                if not (acceptance/(screenshot+'.png')).is_file():
                    errors.append(f'Missing desktop screenshot: {expected_row["observation_id"]}')
                continue
            required_keys = (('baseline_accessible', 'process_blocked', 'positive_blocked',
                              'restored_access', 'restored_process_survived')
                             if cid in ('VM-ACT-001', 'VM-ACT-002') else
                             ('baseline_accessible', 'positive_usage', 'positive_blocked')
                             if cid in ('VM-WEB-008', 'VM-WAL-012') else
                             ('baseline_resolution', 'managed_hosts_blocked',
                              'positive_blocked', 'subdomain_blocked', 'near_miss_accessible')
                             if cid == 'VM-WEB-001' else
                             ('baseline_accessible', 'positive_blocked', 'near_miss_accessible')
                             if cid in ('VM-WEB-003', 'VM-WEB-004', 'VM-WEB-005') else
                             ('open_tab_blocked',) if cid == 'VM-WEB-006' else
                             ('history_navigation_blocked',))
            if (not isinstance(evidence, dict) or not isinstance(evidence.get('rule_id'), str) or
                    not all(evidence.get(key) is True for key in required_keys) or
                    not isinstance(row.get('seconds'), (int, float)) or row['seconds'] < 0 or
                    row['seconds'] > expected_row['budget_seconds']):
                errors.append(f'Assertion evidence or budget failed: {expected_row["observation_id"]}')
            if cid in ('VM-ACT-001', 'VM-ACT-002', 'VM-WEB-008', 'VM-WAL-012') and not lifecycle_assertion_valid(cid, evidence):
                errors.append(f'Incomplete lifecycle evidence: {expected_row["observation_id"]}')
            browser = expected_row['browser_package'].removesuffix('-native')
            if cid == 'VM-WEB-001':
                host = f'{browser}.web.blockuntu.test'
                names = (host, 'www.' + host, f'near-{browser}.web.blockuntu.test')
                if (evidence.get('host_status_compliant') is not True or
                        evidence.get('baseline_addresses') != {name: '127.0.0.1' for name in names} or
                        evidence.get('blocked_addresses') != {
                            names[0]: '0.0.0.0', names[1]: '0.0.0.0', names[2]: '127.0.0.1'}):
                    errors.append(f'Domain resolution transition failed: {expected_row["observation_id"]}')
            suffix = {'VM-WEB-001': 'domain', 'VM-WEB-003': 'url-prefix', 'VM-WEB-004': 'url-contains',
                      'VM-WEB-005': 'path-prefix', 'VM-WEB-006': 'open-tab',
                      'VM-WEB-007': 'spa', 'VM-ACT-001': 'schedule',
                      'VM-ACT-002': 'detox', 'VM-WEB-008': 'allowance', 'VM-WAL-012': 'unlock'}[cid]
            if not (acceptance / f'{browser}-p32-{suffix}.png').is_file():
                errors.append(f'Missing visible browser screenshot: {expected_row["observation_id"]}')
    if selected_ids == HELPER_CASES:
        checkpoint=read_json(acceptance/'helper-result.json')
        if (checkpoint.get('status')!='pass' or not 120<=checkpoint.get('seconds',0)<=240 or
                not all(all(row.get('evidence',{}).get(k)==v for k,v in checkpoint.get('evidence',{}).items())
                        for row in observations)):
            errors.append('Helper supervisor checkpoint, timing or evidence mismatch')
    if selected_ids == {'VM-WEB-001'}:
        dns = read_json(acceptance / 'domain-dns.json')
        required_names = {'web.blockuntu.test'} | {
            name for browser in ('firefox', 'chrome') for name in
            (f'{browser}.web.blockuntu.test', f'www.{browser}.web.blockuntu.test',
             f'near-{browser}.web.blockuntu.test')}
        if (dns.get('public_query_resolved') is not True or
                {name: dns.get('fixture_addresses', {}).get(name) for name in required_names} !=
                {name: '127.0.0.1' for name in required_names} or
                not dns.get('link') or not dns.get('upstream')):
            errors.append('Guest-local DNS baseline/forwarding evidence is incomplete')
    checked['valid'] = not errors
    checked['required_passes'] = len(plan['observations']) if not errors else 0
    checked['smoke_prerequisite_passes'] = len(smoke['observations'])
    return checked


def execute(prepared, *, root=ROOT, invoke=None, checker=None):
    """Stop at first failed group; preserve every emitted result and log."""
    output_dir = prepared['output_dir']
    output_dir.mkdir(mode=0o700, parents=False, exist_ok=False)
    output_dir.chmod(0o700)
    summary_path = output_dir / 'summary.json'
    summary = {'schema_version': 1, 'kind': 'layer3-group-run', 'suite': prepared['plan']['suite'],
               'scope': ('smoke-only' if prepared['plan']['suite'] == 'smoke' else 'selected'),
               'status': 'running', 'package_commit': prepared['commit'],
               'workflow': prepared['workflow'], 'packages': prepared['expected'],
               'executor_sha256': hashlib.sha256(EXECUTOR.read_bytes()).hexdigest(),
               'group_count': len(prepared['runs']),
               'required_observation_count': prepared['groups']['required_observation_count'],
               'runs': prepared['runs'], 'started_at': dt.datetime.now(dt.timezone.utc).isoformat(),
               'live_inventory_check': 'not_performed', 'full_layer3_acceptance': False}
    save_summary(summary_path, summary)
    invoke = invoke or subprocess.run
    checker = checker or verify_bundle
    for item in summary['runs']:
        item['status'] = 'running'
        item['started_at'] = dt.datetime.now(dt.timezone.utc).isoformat()
        save_summary(summary_path, summary)
        command = [sys.executable, str(root / 'scripts/vm-acceptance.py'), '--run-id',
                   item['run_id'], '--guest', item['guest'], '--package-report',
                   prepared['expected'][item['guest']]['report'], '--suite', prepared['plan']['suite']]
        for case_id in item['case_ids']:
            command += ['--case', case_id]
        for scenario in prepared.get('scenarios') or []:
            command += ['--scenario', scenario]
        for browser in prepared.get('browsers') or []:
            command += ['--browser', browser]
        if set(item['case_ids']) == {'VM-WAL-012'} and not prepared.get('browsers'):
            command += ['--browser', prepared['plan']['observations'][0]['browser_package']]
        log_path = output_dir / (item['guest'] +
                                 ('-' + item['case_id'].lower() if item['case_id'] else '') + '.log')
        item['log_path'] = str(log_path)
        try:
            with log_path.open('x') as log:
                result = invoke(command, stdout=log, stderr=subprocess.STDOUT,
                                timeout=item['budget_seconds'] + 900, check=False)
            item['exit_code'] = result.returncode
            checked = checker(item, prepared['expected'][item['guest']], prepared['plan'], root=root)
            item['evidence_check'] = checked
            item['status'] = 'pass' if result.returncode == 0 and checked['valid'] else 'fail'
            if not checked['valid']:
                item['error'] = '; '.join(checked.get('errors', [])) or 'Acceptance evidence failed'
            if result.returncode and checked['valid']:
                item['error'] = 'Executor exited nonzero despite passing evidence'
        except (OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as error:
            item['status'] = 'fail'
            item['error'] = str(error)
        item['finished_at'] = dt.datetime.now(dt.timezone.utc).isoformat()
        save_summary(summary_path, summary)
        if item['status'] != 'pass':
            break
    summary['status'] = ('pass' if all(item['status'] == 'pass' for item in summary['runs'])
                         else 'fail')
    if summary['status'] == 'fail':
        summary['error'] = next((item['error'] for item in summary['runs'] if item.get('error')),
                                'Acceptance group failed')
    summary['finished_at'] = dt.datetime.now(dt.timezone.utc).isoformat()
    save_summary(summary_path, summary)
    return summary_path, summary
