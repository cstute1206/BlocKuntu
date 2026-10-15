#!/usr/bin/env python3
"""Audit complete native P4.2 evidence and two comparable fresh qualification batches."""
import argparse
from layer4_platform import GUESTS, browsers_for, browser_paths, verify_fixture_preparation, hosts_mapping_resolution
import hashlib
import importlib.util
import json
from pathlib import Path

from layer4_hosts_contract import (APPEND_SENTINEL, CASES, FAULTS, HARD, SCENARIOS, SCHEDULED,
    domain_rows, expected_rows, mutated_hosts, save, split_hosts, verify_guest)

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('hosts_runner_audit', Path(__file__).with_name('hosts-acceptance.py'))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def audit(paths, qualify=False):
    if not paths or (qualify and len(paths) != 2):
        raise ValueError('Qualification requires exactly two complete batches')
    identities, manifests, clones, runs, observations = [], [], set(), set(), []
    guest_name = None
    preparation_modes = set()
    for path in paths:
        summary = json.loads(Path(path).read_text())
        selected_guest = summary.get('guest')
        if selected_guest not in GUESTS or (guest_name is not None and selected_guest != guest_name):
            raise ValueError('Unsupported or mixed guest adapters')
        guest_name = selected_guest
        pair = browsers_for(guest_name)
        policies, native_manifests = browser_paths(guest_name)
        suffix = {'ubuntu': '.deb', 'fedora': '.rpm', 'cachyos': '.pkg.tar.zst'}[guest_name]
        if summary.get('status') != 'pass' or summary.get('scope') != guest_name + '-p42' or summary.get('selected_scenarios') != list(SCENARIOS):
            raise ValueError('Complete passing remove/modify/append/delete/allowlist batch required')
        rows = summary.get('runs', [])
        if [row.get('scenario') for row in rows] != list(SCENARIOS):
            raise ValueError('Missing, repeated or reordered hosts scenarios')
        batch_manifest = None
        for row in rows:
            scenario = row['scenario']
            directory = Path(row['evidence']).resolve()
            if not directory.is_relative_to(ROOT / 'results') or directory.name != 'acceptance':
                raise ValueError('Evidence outside owned results tree')
            result = json.loads((directory / 'result.json').read_text())
            preparation_modes.add(verify_fixture_preparation(result, summary, directory, guest_name))
            if (result.get('status') != 'pass' or result.get('scope') != guest_name + '-p42-' + scenario or
                    result.get('collection_errors') or result.get('safety_error') or not result.get('base_integrity_verified') or
                    not result.get('clone_removed') or not result.get('console_access_verified')):
                raise ValueError('Product, evidence or safety audit failed: ' + str(directory))
            if result['run_id'] in runs or result['clone_uuid'] in clones:
                raise ValueError('Every independent fault needs a distinct fresh clone')
            runs.add(result['run_id']); clones.add(result['clone_uuid'])
            owner = json.loads((ROOT / 'runtime/vm-phase0' / result['run_id'] / guest_name / 'owner.json').read_text())
            if (owner['uuid'] != result['clone_uuid'] or owner.get('stage') != 'removed' or not owner.get('paths') or
                    owner.get('template') != guest_name or owner.get('run_id') != result['run_id'] or
                    any(Path(p).exists() for p in owner['paths'])):
                raise ValueError('Missing guarded clone/storage removal evidence')
            report = json.loads((directory / 'package-report.json').read_text())
            artifact = next(a for a in report['artifacts'] if a['filename'].endswith(suffix))
            if (report.get('component') != {'ubuntu': 'deb', 'fedora': 'rpm', 'cachyos': 'arch'}[guest_name] or
                    result.get('guest') != guest_name or not report.get('passed') or not report.get('build_provenance_verified') or
                    report['commit'] != result['package_commit'] or artifact['sha256'] != result['package_sha256'] or
                    report['package_metadata']['version'] != result.get('installed_version')):
                raise ValueError('Candidate provenance differs from installed evidence')
            identities.append((result['package_commit'], result['package_sha256'], result['installed_version']))
            if summary.get('package_commit') != result['package_commit'] or summary.get('package_sha256') != result['package_sha256']:
                raise ValueError('Batch candidate identity differs from its bundles')
            guest = json.loads((directory / 'guest-results.json').read_text())
            count = verify_guest(guest, scenario, guest_name)
            if count != result.get('required_passes'):
                raise ValueError('Pass count differs from required observations')
            manifest = json.loads((directory / 'harness-files.json').read_text())
            if set(manifest) != set(runner.SOURCE_PATHS):
                raise ValueError('Missing or unexpected captured sources')
            for name, digest in manifest.items():
                source = directory / 'harness' / name
                if source.is_symlink() or not source.resolve().is_relative_to(directory / 'harness'):
                    raise ValueError('Invalid captured source path')
                if hashlib.sha256(source.read_bytes()).hexdigest() != digest:
                    raise ValueError('Captured source checksum differs: ' + name)
            if batch_manifest is not None and manifest != batch_manifest:
                raise ValueError('Harness changed within a batch')
            batch_manifest = manifest
            required = ['baseline/baseline.json', 'baseline/export.json', 'baseline/seeded.json', 'inspection.json',
                        'collector.json', 'journal.log', 'collector.log', 'suite.log', 'network.json', 'p42-console-preflight.png']
            if scenario != 'allowlist':
                required += ['before-fault/inspection.json', 'after-fault/inspection.json', 'p42-fault.json', 'p42-repaired.json']
            if scenario == 'remove':
                required += ['p42-schedule.json', 'schedule-inactive/inspection.json', 'schedule-active/inspection.json', 'schedule-ended/inspection.json']
            if scenario == 'allowlist':
                required += ['p42-allowlist.json', 'allowlist-active/inspection.json']
            for name in required:
                if not (directory / name).is_file():
                    raise ValueError('Required evidence missing: ' + name)
                if name.endswith('.json'):
                    json.loads((directory / name).read_text())
            def hosts_checkpoint(name, domains):
                state = json.loads((directory / name).read_text())
                block, external = split_hosts(state['hosts'])
                flags = state['hosts_attributes']['stdout'].split()
                if (domain_rows(block) != expected_rows(domains) or
                        state['hosts_attributes']['returncode'] != 0 or not flags or 'i' not in flags[0]):
                    raise ValueError('Independent hosts checkpoint contradicts pass: ' + name)
            if scenario != 'allowlist':
                fault = json.loads((directory / 'p42-fault.json').read_text())
                expected = None if scenario == 'delete' else mutated_hosts(fault['before']['hosts'], FAULTS[scenario])
                if (fault.get('name') != FAULTS[scenario] or fault.get('fault_witnessed') is not True or
                        fault.get('immutable_clear_witnessed') is not True or fault.get('fault_hosts') != expected):
                    raise ValueError('Independent root fault witness contradicts pass')
                hosts_checkpoint('after-fault/inspection.json', (HARD, 'www.' + HARD))
            if scenario == 'append':
                repaired = json.loads((directory / 'p42-repaired.json').read_text())
                resolution = repaired.get('details', {}).get('sentinel_resolution', {})
                samples = resolution.get('samples', [])
                if (resolution.get('expected_address') != '127.0.0.43' or
                        resolution.get('timeout_seconds') != 30 or
                        resolution.get('settled') is not True or
                        not isinstance(samples, list) or not samples or
                        not hosts_mapping_resolution(samples[-1], '127.0.0.43')):
                    raise ValueError('Append sentinel resolver witness contradicts pass')
                after = json.loads((directory / 'after-fault/inspection.json').read_text())
                _, outside = split_hosts(after['hosts'])
                if APPEND_SENTINEL not in outside.splitlines():
                    raise ValueError('Append sentinel external checkpoint contradicts pass')
            if scenario == 'remove':
                for stage, domains in [('inactive',(HARD,'www.'+HARD)), ('active',(HARD,'www.'+HARD,SCHEDULED)), ('ended',(HARD,'www.'+HARD))]:
                    hosts_checkpoint('schedule-' + stage + '/inspection.json', domains)
            if scenario == 'allowlist':
                hosts_checkpoint('allowlist-active/inspection.json', ())
            observations.append({'scenario': scenario, 'run_id': result['run_id'], 'required_passes': count,
                                 'clone_uuid': result['clone_uuid'], 'evidence': str(directory)})
        manifests.append(batch_manifest)
    if len(set(identities)) != 1:
        raise ValueError('Candidate changed between bundles')
    if qualify and manifests[0] != manifests[1]:
        raise ValueError('Qualification requires identical captured harness versions')
    if qualify and len(preparation_modes) != 1:
        raise ValueError('Qualification requires comparable rolling fixture preparation')
    return {'status': 'pass', 'scope': guest_name + '-p42', 'adapter_qualified': qualify, 'batch_count': len(paths),
            'package_commit': identities[0][0], 'package_sha256': identities[0][1], 'installed_version': identities[0][2],
            'required_passes': sum(row['required_passes'] for row in observations), 'observations': observations,
            'all_owned_clones_removed': True, 'full_layer4_acceptance': False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('summaries', nargs='+', type=Path)
    parser.add_argument('--qualify', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = audit(args.summaries, args.qualify)
    except Exception as error:
        result = {'status': 'fail', 'scope': 'native-p42', 'adapter_qualified': False, 'error': str(error)}
    save(args.output, result)
    print(result['status'], result.get('required_passes', ''), result.get('error', ''), args.output)
    return int(result['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
