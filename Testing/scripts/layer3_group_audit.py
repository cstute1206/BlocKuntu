"""Recheck saved Layer 3 smoke groups and, optionally, current libvirt state."""
from __future__ import annotations

import datetime as dt
import importlib.util
import json
from pathlib import Path

from layer3_contract import ROOT, build_plan, load_contract
from layer3_group_runner import read_json, verify_bundle


def phase0_module(root):
    spec = importlib.util.spec_from_file_location('layer3_audit_phase0', root / 'scripts/phase0-vm.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def timestamp(value):
    parsed = dt.datetime.fromisoformat(value)
    if parsed.tzinfo is None:
        raise ValueError('Group timestamp lacks a timezone')
    return parsed


def audit_summaries(paths, *, guests=None, live=False, root=ROOT):
    """Require the latest attempted group per guest to pass, including cleanup."""
    if not paths or len({str(Path(path).resolve()) for path in paths}) != len(paths):
        raise ValueError('Provide distinct saved group summaries')
    registry, matrix = load_contract(root)
    guests = registry['guests'] if guests is None else list(guests)
    if not guests or len(guests) != len(set(guests)) or not set(guests) <= set(registry['guests']):
        raise ValueError('Invalid audit guest selection')
    attempts = []
    commits = set()
    for path in paths:
        summary = read_json(path)
        if (summary.get('kind') != 'layer3-group-run' or summary.get('suite') != 'smoke' or
                summary.get('scope') != 'smoke-only' or summary.get('status') not in ('pass', 'fail')):
            raise ValueError(f'Not a completed smoke-group summary: {path}')
        commit = summary.get('package_commit')
        if summary.get('workflow', {}).get('headSha') != commit:
            raise ValueError(f'Group workflow/source commit differs: {path}')
        commits.add(commit)
        for item in summary.get('runs', []):
            guest = item.get('guest')
            if guest not in registry['guests'] or item.get('group_id') != f'{guest}:smoke-baseline':
                raise ValueError(f'Unknown group in {path}')
            if item.get('status') == 'not_run':
                continue
            if item.get('status') not in ('pass', 'fail'):
                raise ValueError(f'Unfinished group in {path}')
            start, finish = timestamp(item['started_at']), timestamp(item['finished_at'])
            if finish <= start:
                raise ValueError(f'Invalid group interval in {path}')
            expected = summary.get('packages', {}).get(guest)
            if not expected or expected.get('commit') != commit:
                raise ValueError(f'Group package provenance missing for {guest}')
            attempts.append({'item': item, 'guest': guest, 'started': start, 'finished': finish,
                             'expected': expected, 'summary_path': str(Path(path).resolve())})
    if len(commits) != 1 or len({a['item']['run_id'] for a in attempts}) != len(attempts):
        raise ValueError('Group summaries mix source commits or reuse run IDs')
    by_guest = {guest: sorted((a for a in attempts if a['guest'] == guest),
                              key=lambda a: a['started']) for guest in guests}
    if any(not rows or rows[-1]['item']['status'] != 'pass' for rows in by_guest.values()):
        raise ValueError('The latest attempted group did not pass for every selected guest')
    selected = [by_guest[guest][-1] for guest in guests]
    checked = []
    for attempt in selected:
        item, guest = attempt['item'], attempt['guest']
        plan = build_plan(registry, matrix, suite='smoke', guests=[guest])
        evidence = verify_bundle(item, attempt['expected'], plan, root=root)
        if item.get('exit_code') != 0 or not evidence['valid'] or evidence['required_passes'] != 21:
            raise ValueError(f"Saved smoke evidence failed recheck: {item['run_id']}: {evidence['errors']}")
        checked.append({'guest': guest, 'run_id': item['run_id'],
                        'summary_path': attempt['summary_path'], 'evidence': evidence,
                        'started_at': attempt['started'].isoformat(),
                        'finished_at': attempt['finished'].isoformat()})
    for field in ('clone_uuid', 'machine_id', 'host_fingerprint'):
        if len({row['evidence'][field] for row in checked}) != len(checked):
            raise ValueError(f'Successful groups reused {field}')
    if len({json.dumps(row['evidence']['manifest'], sort_keys=True) for row in checked}) != 1:
        raise ValueError('Successful groups used different smoke harness manifests')
    ordered = sorted(attempts, key=lambda a: a['started'])
    if any(a['finished'] > b['started'] for a, b in zip(ordered, ordered[1:])):
        raise ValueError('Group attempts overlapped')
    owners = {}
    for attempt in attempts:
        item, guest = attempt['item'], attempt['guest']
        owner_path = root / 'runtime/vm-phase0' / item['run_id'] / guest / 'owner.json'
        if owner_path.exists():
            owner = read_json(owner_path)
            if (owner.get('run_id') != item['run_id'] or owner.get('template') != guest or
                    owner.get('stage') != 'removed' or not owner.get('paths') or
                    any(Path(path).exists() for path in owner['paths'])):
                raise ValueError(f'Attempt has an owned clone or copied storage: {item["run_id"]}')
            owners[item['run_id']] = owner
        elif item['status'] == 'pass':
            raise ValueError(f'Passing group lacks clone ownership: {item["run_id"]}')
    inventory = 'not_performed'
    domains = None
    if live:
        vm = phase0_module(root)
        vm.base_records()
        for owner in owners.values():
            vm.unchanged(owner)
        domains = [name for name in vm.virsh('list', '--all', '--name').stdout.splitlines() if name]
        expected_domains = {item['domain'] for item in vm.CONFIG['templates'].values()}
        if set(domains) != expected_domains:
            raise ValueError(f'Live libvirt domains differ from the immutable templates: {domains}')
        states = {name: vm.virsh('domstate', name).stdout.strip() for name in domains}
        if any(state != 'shut off' for state in states.values()):
            raise ValueError(f'A base VM is not shut off: {states}')
        uuids = set(vm.virsh('list', '--all', '--uuid').stdout.splitlines())
        if {owner['uuid'] for owner in owners.values()} & uuids:
            raise ValueError('An owned clone still exists in libvirt')
        inventory = 'pass'
        domains = states
    return {'schema_version': 1, 'kind': 'layer3-group-audit',
            'scope': 'smoke-only', 'status': 'pass' if live else 'incomplete',
            'package_commit': commits.pop(), 'required_observations_passed': 21 * len(guests),
            'latest_successful_runs': checked,
            'earlier_attempts': [{'guest': attempt['guest'], 'run_id': attempt['item']['run_id'],
                                  'status': attempt['item']['status'],
                                  'summary_path': attempt['summary_path']}
                                 for attempt in attempts if attempt not in selected],
            'same_harness_manifest': True, 'serial_execution': True,
            'distinct_clone_identities': True, 'copied_storage_removed': True,
            'live_inventory_check': inventory, 'domains': domains,
            'full_layer3_acceptance': False,
            'checked_at': dt.datetime.now(dt.timezone.utc).isoformat()}
