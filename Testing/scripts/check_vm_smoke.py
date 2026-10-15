#!/usr/bin/env python3
"""Check saved consecutive smoke pairs; never start, change, or remove a VM."""
from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime
import json
from pathlib import Path
import re

from layer3_contract import ROOT, build_plan, load_contract


def check_run(result, package, manifest, browsers, owner, phase0, expected, required):
    """Validate an existing smoke evidence bundle, not full Layer 3 acceptance."""
    errors = []
    guest, run_id = result.get('guest'), result.get('run_id')
    if guest != expected['guest'] or result.get('suite') != f'{guest}-smoke':
        errors.append('Guest/suite differs from the selected smoke contract')
    if result.get('status') != 'pass' or result.get('base_integrity_verified') is not True:
        errors.append('Acceptance did not pass with base integrity verified')
    if result.get('error') or result.get('cleanup_error'):
        errors.append('Acceptance contains a setup or cleanup error')
    if package.get('passed') is not True or package.get('build_provenance_verified') is not True:
        errors.append('Package report is not passed and provenance-verified')
    if result.get('package_commit') != expected['commit'] or package.get('commit') != expected['commit']:
        errors.append('Package source commit differs from the selected artifact')
    if result.get('package') != package.get('artifacts'):
        errors.append('Acceptance/package-report artifacts differ')
    selected = [a for a in package.get('artifacts', []) if a.get('filename') == expected['filename']]
    if len(selected) != 1 or selected[0].get('sha256') != expected['sha256']:
        errors.append('Selected package checksum is absent or differs')
    if package.get('package_metadata', {}).get('version') != expected['version']:
        errors.append('Selected package version differs')
    if (owner.get('run_id') != run_id or owner.get('template') != guest or
            owner.get('stage') != 'removed' or not owner.get('uuid') or not owner.get('bases')):
        errors.append('Clone removal/ownership evidence is missing or inconsistent')
    if (phase0.get('status') != 'passed' or phase0.get('clone_uuid') != owner.get('uuid') or
            not phase0.get('machine_id') or not phase0.get('host_fingerprint')):
        errors.append('Fresh-clone identity evidence is missing or inconsistent')
    essential = {'scripts/vm-acceptance.py', 'scripts/phase0-vm.py',
                 'fixtures/layer3/guest.py', 'fixtures/layer3/browsers.py',
                 'fixtures/layer3/processes.py', 'fixtures/layer3/gui.py'}
    if not essential <= set(manifest) or not all(re.fullmatch(r'[0-9a-f]{64}', h) for h in manifest.values()):
        errors.append('Harness manifest is incomplete or invalid')
    if result.get('harness_sha256') != manifest.get('scripts/vm-acceptance.py'):
        errors.append('Harness result hash differs from its manifest')
    seen = Counter()
    passed = set()
    for row in result.get('cases', []):
        key = (row.get('id'), row.get('browser'))
        seen[key] += 1
        if key in required:
            if row.get('status') == 'pass':
                passed.add(key)
            else:
                errors.append(f'Required observation did not pass: {key}')
        elif row.get('id') == 'VM-UPGRADE-001' and row.get('status') == 'skip' and row.get('reason'):
            pass
        elif row.get('status') != 'planned' or row.get('browser') is not None:
            errors.append(f'Unexpected observation/outcome: {key}')
        if row.get('id') == 'VM-INSTALL-001' and row.get('version') != expected['version']:
            errors.append('Installed package version differs')
    if any(count != 1 for count in seen.values()):
        errors.append('Duplicate observation keys')
    if passed != required:
        errors.append('Missing required observations: ' + str(sorted(required - passed, key=str)))
    expected_browsers = {browser for _, browser in required if browser}
    if set(browsers) != expected_browsers:
        errors.append('Browser metadata does not match the required browser set')
    version_fields = ('browser_version', 'extension_version', 'extension_id', 'package_source', 'policy_path')
    versions = {}
    for browser, metadata in browsers.items():
        if any(not metadata.get(field) for field in version_fields):
            errors.append(f'Incomplete browser metadata: {browser}')
        versions[browser] = {field: metadata.get(field) for field in version_fields}
        for row in result.get('cases', []):
            if row.get('browser') == browser and row.get('id') in ('VM-BR-001', 'VM-BR-002'):
                heartbeat = row.get('evidence', {}).get('heartbeat', {})
                if heartbeat.get('extension_version') != metadata.get('extension_version'):
                    errors.append(f'Extension version changed or heartbeat evidence missing: {browser}')
    try:
        start, finish = (datetime.fromisoformat(result[field]) for field in ('started_at', 'finished_at'))
        if start.tzinfo is None or finish.tzinfo is None or finish <= start:
            raise ValueError('Invalid time interval')
        minutes = round((finish - start).total_seconds() / 60, 1)
    except (KeyError, TypeError, ValueError):
        errors.append('Missing or invalid run timestamps')
        minutes = None
    return {'run_id': run_id, 'guest': guest, 'valid': not errors, 'errors': errors,
            'started_at': result.get('started_at'), 'finished_at': result.get('finished_at'),
            'minutes': minutes, 'required_passes': len(passed), 'manifest': manifest,
            'package_sha256': expected['sha256'], 'browser_versions': versions,
            'clone_uuid': owner.get('uuid'), 'machine_id': phase0.get('machine_id'),
            'host_fingerprint': phase0.get('host_fingerprint'), 'bases': owner.get('bases')}


def check_pair(runs):
    """Use the latest two chronological attempts; retain earlier failed history."""
    if len(runs) < 2:
        return {'status': 'incomplete', 'reason': 'Two completed attempts are required'}
    if any(not run.get('started_at') for run in runs):
        return {'status': 'incomplete', 'reason': 'An attempt has incomplete evidence'}
    try:
        pair = sorted(runs, key=lambda run: datetime.fromisoformat(run['started_at']))[-2:]
    except (ValueError, TypeError):
        return {'status': 'fail', 'reason': 'Invalid attempt timestamps'}
    ids = [run['run_id'] for run in pair]
    if any(not run['valid'] for run in pair):
        return {'status': 'fail', 'runs': ids, 'reason': 'Latest two attempts did not both validate'}
    for field in ('run_id', 'clone_uuid', 'machine_id', 'host_fingerprint'):
        if pair[0][field] == pair[1][field]:
            return {'status': 'fail', 'runs': ids, 'reason': f'Fresh-clone {field} was reused'}
    for field in ('guest', 'manifest', 'package_sha256', 'browser_versions', 'bases'):
        if pair[0][field] != pair[1][field]:
            return {'status': 'fail', 'runs': ids, 'reason': f'Pair differs in {field}'}
    if datetime.fromisoformat(pair[0]['finished_at']) > datetime.fromisoformat(pair[1]['started_at']):
        return {'status': 'fail', 'runs': ids, 'reason': 'Runs overlapped instead of executing serially'}
    return {'status': 'pass', 'runs': ids, 'reason': 'Two comparable fresh-clone smoke passes'}


def unreadable_attempt(run_id, guest, error, result=None):
    """Retain terminal failures in chronology even when their bundle is partial."""
    result = result or {}
    started = None
    if (result.get('run_id') == run_id and result.get('guest') == guest and
            result.get('status') in ('pass', 'fail', 'incomplete')):
        try:
            start, finish = (datetime.fromisoformat(result[field]) for field in ('started_at', 'finished_at'))
            if start.tzinfo is not None and finish.tzinfo is not None and finish > start:
                started = result['started_at']
        except (KeyError, TypeError, ValueError):
            pass
    return {'run_id': run_id, 'guest': guest, 'valid': False,
            'started_at': started, 'errors': [str(error)]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', required=True, help='Run series prefix, e.g. p30-36269349435')
    parser.add_argument('--verified-inputs', required=True, type=Path)
    parser.add_argument('--output', type=Path, help='Write a new summary file; never overwrite evidence')
    args = parser.parse_args()
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,39}', args.prefix):
        parser.error('Invalid run-series prefix')
    inputs = json.loads(args.verified_inputs.read_text())
    registry, matrix = load_contract()
    guests = registry['guests']
    expected = {item['guest']: {**item, 'filename': Path(item['package']).name,
                'commit': inputs['workflow']['headSha']} for item in inputs['packages']}
    if set(expected) != set(guests) or len(inputs['packages']) != len(guests):
        parser.error('Verified input record must contain exactly the three selected guests')
    runs = {guest: [] for guest in guests}
    for directory in sorted((ROOT / 'results').glob(args.prefix + '-*')):
        if not directory.is_dir():
            continue
        for guest in guests:
            base = directory / guest
            if not base.is_dir():
                continue
            acceptance = base / 'acceptance'
            required = {(row['id'], row['browser_package'].removesuffix('-native')
                         if row['browser_package'] else None)
                        for row in build_plan(registry, matrix, guests=[guest])['observations']}
            result = None
            try:
                def read(path):
                    return json.loads(path.read_text())
                result = read(acceptance / 'result.json')
                if result.get('run_id') != directory.name or result.get('guest') != guest:
                    raise ValueError('Result run ID/guest does not match its evidence directory')
                checked = check_run(result, read(acceptance / 'package-report.json'),
                    read(acceptance / 'harness-files.json'), read(acceptance / 'browser-metadata.json'),
                    read(ROOT / 'runtime/vm-phase0' / directory.name / guest / 'owner.json'),
                    read(base / 'result.json'), expected[guest], required)
            except (OSError, ValueError, KeyError, TypeError) as error:
                checked = unreadable_attempt(directory.name, guest, error, result)
            checked['result_path'] = str(acceptance / 'result.json')
            runs[guest].append(checked)
    pairs = {guest: check_pair(attempts) for guest, attempts in runs.items()}
    # This summary only verifies saved smoke evidence. The final operator must
    # also verify current libvirt inventory; this command never contacts libvirt.
    summary = {'scope': 'P3.0-smoke-repeatability', 'package_commit': inputs['workflow']['headSha'],
               'pairs': pairs, 'runs': runs, 'live_inventory_check': 'not_performed',
               'status': 'pass' if all(pair['status'] == 'pass' for pair in pairs.values()) else 'incomplete'}
    if any(pair['status'] == 'fail' for pair in pairs.values()):
        summary['status'] = 'fail'
    if args.output:
        with args.output.open('x') as destination:
            destination.write(json.dumps(summary, indent=2) + '\n')
    print(json.dumps({'status': summary['status'], 'pairs': pairs}, indent=2))
    return int(summary['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
