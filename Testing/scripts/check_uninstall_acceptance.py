#!/usr/bin/env python3
"""Audit complete P4.1 bundles; optionally qualify two comparable fresh passes."""
import argparse
from layer4_platform import GUESTS, browsers_for, browser_paths, verify_fixture_preparation
import hashlib
import json
from pathlib import Path

from layer4_uninstall_contract import SCENARIOS, native_removal_witness, handoff_heartbeats, installed_packages_preserved, save, verify_guest

ROOT = Path(__file__).resolve().parents[1]


def audit(summary_paths, qualify=False):
    if qualify and len(summary_paths) != 2:
        raise ValueError('Adapter qualification requires exactly two complete batches')
    manifests, artifacts, versions = [], [], []
    clones, run_ids, observations = set(), set(), []
    guest_name = None
    preparation_modes = set()
    for path in summary_paths:
        summary = json.loads(Path(path).read_text())
        selected_guest = summary.get('guest')
        if selected_guest not in GUESTS or (guest_name is not None and selected_guest != guest_name):
            raise ValueError('Unsupported or mixed guest adapters')
        guest_name = selected_guest
        pair = browsers_for(guest_name)
        policies, native_manifests = browser_paths(guest_name)
        suffix = {'ubuntu': '.deb', 'fedora': '.rpm', 'cachyos': '.pkg.tar.zst'}[guest_name]
        if summary.get('status') != 'pass' or summary.get('selected_scenarios') != list(SCENARIOS):
            raise ValueError('A complete passing remove/purge/authorized batch is required')
        rows = summary.get('runs', [])
        if [r.get('scenario') for r in rows] != list(SCENARIOS):
            raise ValueError('Missing, repeated or reordered scenario bundles')
        batch_manifest = None
        for row in rows:
            directory = Path(row['evidence']).resolve()
            if not directory.is_relative_to(ROOT / 'results') or directory.name != 'acceptance':
                raise ValueError('Evidence outside the owned results tree')
            result = json.loads((directory / 'result.json').read_text())
            preparation_modes.add(verify_fixture_preparation(result, summary, directory, guest_name))
            if (result.get('status') != 'pass' or result.get('collection_errors') or
                    not result.get('base_integrity_verified') or not result.get('clone_removed') or
                    not result.get('console_access_verified') or result.get('safety_error')):
                raise ValueError('Product, evidence or safety audit failed: ' + str(directory))
            if result['run_id'] in run_ids or result['clone_uuid'] in clones:
                raise ValueError('Each scenario must use a distinct fresh clone/run')
            run_ids.add(result['run_id'])
            clones.add(result['clone_uuid'])
            owner = json.loads((ROOT / 'runtime/vm-phase0' / result['run_id'] / guest_name / 'owner.json').read_text())
            if (owner['uuid'] != result['clone_uuid'] or owner.get('stage') != 'removed' or
                    owner.get('template') != guest_name or owner.get('run_id') != result['run_id'] or
                    not owner.get('paths') or any(Path(p).exists() for p in owner['paths'])):
                raise ValueError('Missing guarded storage-removal evidence')
            report = json.loads((directory / 'package-report.json').read_text())
            artifact = next(a for a in report['artifacts'] if a['filename'].endswith(suffix))
            if (report.get('component') != {'ubuntu': 'deb', 'fedora': 'rpm', 'cachyos': 'arch'}[guest_name] or
                    result.get('guest') != guest_name or not report.get('passed') or not report.get('build_provenance_verified') or
                    report['commit'] != result['package_commit'] or artifact['sha256'] != result['package_sha256'] or
                    report['package_metadata']['version'] != result.get('installed_version')):
                raise ValueError('Candidate provenance differs from the installed evidence')
            identity = (result['package_commit'], result['package_sha256'], result['installed_version'])
            artifacts.append(identity)
            guest = json.loads((directory / 'guest-results.json').read_text())
            count = verify_guest(guest, row['scenario'], guest_name)
            if count != result.get('required_passes'):
                raise ValueError('Pass count differs from audited observations')
            manifest = json.loads((directory / 'harness-files.json').read_text())
            for name, digest in manifest.items():
                target = directory / 'harness' / name
                if not target.resolve().is_relative_to(directory / 'harness') or target.is_symlink():
                    raise ValueError('Invalid captured-source path')
                if hashlib.sha256(target.read_bytes()).hexdigest() != digest:
                    raise ValueError('Captured harness checksum mismatch: ' + name)
            if batch_manifest is not None and batch_manifest != manifest:
                raise ValueError('Harness changed within a batch')
            batch_manifest = manifest
            for required in ('baseline/baseline.json', 'baseline/export.json', 'baseline/seeded.json',
                             'inspection.json', 'collector.json', 'journal.log', 'collector.log', 'suite.log', ('dpkg.log' if guest_name == 'ubuntu' else 'native-package.log')):
                if not (directory / required).is_file():
                    raise ValueError('Required evidence missing: ' + required)
            if row['scenario'] != 'authorized' and guest_name != 'ubuntu':
                before = json.loads((directory / 'baseline/inspection.json').read_text())
                after = json.loads((directory / 'after-uninstall/inspection.json').read_text())
                if not installed_packages_preserved(before.get('package_inventory'), after.get('package_inventory')):
                    raise ValueError('Rejected transaction changed the installed package inventory')
                for label, key in (('baseline', 'gui_baseline'), ('post-refusal', 'gui_post_refusal')):
                    proof = json.loads((directory / ('p41-gui-' + label + '.json')).read_text())
                    if (proof != guest.get(key) or proof.get('label') != label or
                            proof.get('ordinary_uid') != guest['ordinary_uid'] or
                            proof.get('unit') != 'blockuntu-p41-gui-' + label or
                            any(proof.get(k) is not True for k in
                                ('gui_restart_healthy', 'settings_opened', 'settings_closed')) or
                            not proof.get('journal')):
                        raise ValueError('Native GUI did not survive rejected removal')
            if row['scenario'] == 'authorized':
                for required in ('p41-before-uninstall.json', 'p41-after.json', 'p41-cleanup.json',
                                 'p41-handoff.json', 'p41-firefox-recovered.png', 'p41-' + pair[1] + '-recovered.png'):
                    if not (directory / required).is_file():
                        raise ValueError('Authorized-uninstall evidence missing: ' + required)
                handoff = json.loads((directory / 'p41-handoff.json').read_text())
                if any('handoff' in sample for sample in handoff):
                    if not all(handoff_heartbeats(handoff, pair).values()):
                        raise ValueError('Independent uninstall heartbeat witness missing')
                if guest_name != 'ubuntu':
                    logs = json.loads((directory / 'native-package.log').read_text())
                    if not native_removal_witness(logs, guest['uninstall_requested_at'],
                                                  result['installed_version'], guest_name):
                        raise ValueError('Independent native transaction witness missing')
                if [b['browser'] for b in guest['browsers']] != list(pair):
                    raise ValueError('Native browser pair differs from selected adapter')
                versions.append([(b['browser'], b['browser_version'], b['heartbeat']['extension_version']) for b in guest['browsers']])
            observations.append({'run_id': result['run_id'], 'scenario': row['scenario'], 'required_passes': count,
                                 'clone_uuid': result['clone_uuid'], 'evidence': str(directory)})
        manifests.append(batch_manifest)
    if len(set(artifacts)) != 1:
        raise ValueError('Candidate changed between scenario bundles')
    if qualify and (manifests[0] != manifests[1] or versions[0] != versions[1]):
        raise ValueError('Qualification requires comparable harness/browser/extension versions')
    if qualify and len(preparation_modes) != 1:
        raise ValueError('Qualification requires comparable rolling fixture preparation')
    return {'status': 'pass', 'scope': guest_name + '-p41', 'adapter_qualified': qualify,
            'batch_count': len(summary_paths), 'package_commit': artifacts[0][0],
            'package_sha256': artifacts[0][1], 'installed_version': artifacts[0][2],
            'required_passes': sum(r['required_passes'] for r in observations),
            'observations': observations, 'all_owned_clones_removed': True,
            'full_layer4_acceptance': False}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('summaries', nargs='+', type=Path)
    p.add_argument('--qualify', action='store_true')
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    try:
        result = audit(args.summaries, args.qualify)
    except Exception as error:
        result = {'status': 'fail', 'scope': 'native-p41', 'error': str(error), 'adapter_qualified': False}
    save(args.output, result)
    print(result['status'], result.get('required_passes', ''), result.get('error', ''), args.output)
    return int(result['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
