#!/usr/bin/env python3
"""Audit complete native P4.3 evidence and two comparable fresh qualification batches."""
import argparse
from layer4_platform import GUESTS, browsers_for, browser_paths, verify_fixture_preparation
import datetime as dt
import hashlib
import importlib.util
import json
from pathlib import Path

from layer4_browser_contract import (CASES, MANIFESTS, POLICIES, SCENARIOS,
    fault_paths, diagnostic_witness, installed_timing, observation_seconds, verify_crash_pause, save, verify_guest)

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('browser_runner_audit', Path(__file__).with_name('browser-acceptance.py'))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def manifest_disconnect_counts(log, not_before, not_after, pair=('firefox', 'chrome'), heartbeats=None):
    """Queued diagnostics must prove reconnect failures while damage existed."""
    end = dt.datetime.fromisoformat(not_after)
    counts = {browser: sum(
        detail['kind'] == 'native_host_disconnected' and
        dt.datetime.fromisoformat(detail['observed_at'].replace('Z', '+00:00')) < end
        for detail in diagnostic_witness(log, browser, not_before,
            None if heartbeats is None else heartbeats[browser]))
        for browser in pair}
    if any(count < 2 for count in counts.values()):
        raise ValueError('Repeated native reconnect failures during manifest damage required for both browsers')
    return counts


def audit(paths, qualify=False):
    if not paths or (qualify and len(paths) != 2):
        raise ValueError('Qualification requires exactly two complete batches')
    identities, extension_identities, manifests, clones, runs, observations = [], [], [], set(), set(), []
    guest_name = None
    preparation_modes = set()
    for path in paths:
        summary = json.loads(Path(path).read_text())
        selected_guest = summary.get('guest')
        if selected_guest not in GUESTS or (guest_name is not None and selected_guest != guest_name):
            raise ValueError('Unsupported or mixed guest adapters')
        guest_name = selected_guest
        pair = browsers_for(guest_name)
        policies, native_manifests = fault_paths(guest_name)
        suffix = {'ubuntu': '.deb', 'fedora': '.rpm', 'cachyos': '.pkg.tar.zst'}[guest_name]
        if summary.get('status') != 'pass' or summary.get('scope') != guest_name + '-p43' or summary.get('selected_scenarios') != list(SCENARIOS):
            raise ValueError('Complete passing policy/manifest/crash/outage batch required')
        rows = summary.get('runs', [])
        if [row.get('scenario') for row in rows] != list(SCENARIOS):
            raise ValueError('Missing, repeated or reordered browser scenarios')
        batch_manifest = None
        for row in rows:
            scenario = row['scenario']
            directory = Path(row['evidence']).resolve()
            if not directory.is_relative_to(ROOT / 'results') or directory.name != 'acceptance':
                raise ValueError('Evidence outside owned results tree')
            result = json.loads((directory / 'result.json').read_text())
            preparation_modes.add(verify_fixture_preparation(result, summary, directory, guest_name))
            if (row.get('status') != 'pass' or not row.get('base_integrity_verified') or not row.get('clone_removed') or
                    result.get('status') != 'pass' or result.get('scope') != guest_name + '-p43-' + scenario or
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
            required = ['baseline/baseline.json', 'baseline/export.json', 'baseline/seeded.json',
                'baseline/extensions.json', 'inspection.json', 'collector.json', 'collector-history.jsonl',
                'journal.log', 'collector.log', 'suite.log', 'browser-journal.log','event.log','p43-console-preflight.png',
                'before-fault/inspection.json', 'before-fault/p43-plan.json', 'during-fault/inspection.json',
                'recovered/inspection.json', 'p43-fault.json', 'p43-observed.json', 'p43-recovery.json']
            if scenario in ('manifest','outage'):
                required += ['p43-restored.json']
            if scenario in ('manifest','crash'):
                required += ['p43-diagnostics.json']
            if scenario == 'manifest':
                required += ['p43-health-trace.json']
            for name in required:
                if not (directory / name).is_file():
                    raise ValueError('Required evidence missing: ' + name)
                if name.endswith('.json'):
                    json.loads((directory / name).read_text())
            def read(name):
                return json.loads((directory / name).read_text())
            baseline = read('baseline/baseline.json')
            codes = read('baseline/extensions.json')
            if [r['browser'] for r in codes] != list(pair):
                raise ValueError('Both published installed extensions are required')
            extension_identities.append(json.dumps([
                (c['browser'], next(b['browser_version'] for b in baseline['browsers']
                    if b['browser'] == c['browser']), c['manifest']['version'], c['script_sha256'])
                for c in codes], sort_keys=True))
            for code in codes:
                if any(hashlib.sha256(text.encode()).hexdigest() != code['script_sha256'][name]
                       for name,text in code['scripts'].items()):
                    raise ValueError('Installed extension source checksum differs')
                if installed_timing('\n'.join(code['scripts'].values())) != code['timing']:
                    raise ValueError('Installed timing differs from captured extension code')
                browser = next(r for r in baseline['browsers'] if r['browser']==code['browser'])
                if not browser.get('browser_version'):
                    raise ValueError('Installed native browser version is missing')
                if code['manifest']['version'] != browser['heartbeat']['extension_version']:
                    raise ValueError('Installed extension identity differs from baseline heartbeat')
            fault, observed = read('p43-fault.json'), read('p43-observed.json')
            during, recovered = read('during-fault/inspection.json'), read('recovered/inspection.json')
            if fault.get('scenario') != scenario or fault.get('fault_witnessed') is not True:
                raise ValueError('Independent fault witness contradicts pass')
            if scenario == 'policy':
                for path in policies:
                    if json.loads(fault['damaged_files'][path]) == json.loads(fault['before']['files'][path]):
                        raise ValueError('Policy was not damaged')
                    if json.loads(during['files'][path]) != json.loads(fault['before']['files'][path]):
                        raise ValueError('Managed policy was not restored exactly')
            elif scenario == 'manifest':
                if not fault.get('connections_terminated') or len(fault['before']['native_processes']) < 2:
                    raise ValueError('Original native connections were not terminated')
                for path in native_manifests:
                    if json.loads(during['files'][path])['path'] != '/nonexistent/blockuntu-p43-native':
                        raise ValueError('Manifest damage ended before degradation observation')
                    if recovered['files'][path] != fault['before']['files'][path]:
                        raise ValueError('Original packaged manifest was not restored')
            elif scenario == 'crash':
                verify_crash_pause(fault, [code['timing'] for code in codes])
                if during['daemon']['pid'] == fault['before']['daemon']['pid']:
                    raise ValueError('Daemon did not restart')
                if not all(diagnostic_witness(read('p43-diagnostics.json')['log'], b, fault['before']['checked_at'],
                    next(row['heartbeat'] for row in baseline['browsers'] if row['browser']==b)) for b in pair):
                    raise ValueError('Missing current-fault browser transport interruption diagnostics')
            else:
                if during['daemon']['state'] != 'T' or recovered['daemon']['state'] == 'T':
                    raise ValueError('No sustained daemon pause/recovery witnessed')
                if observed['details']['observed_duration_seconds'] < max(observation_seconds(c['timing']) for c in codes):
                    raise ValueError('Outage observation shorter than installed tolerance')
                if not observed['details']['rpc_failures']:
                    raise ValueError('Actual failed daemon communication is required')
            if scenario == 'manifest':
                details = observed['details']
                manifest_disconnect_counts((directory / 'event.log').read_text(),
                    fault['before']['checked_at'], read('p43-restored.json')['inspection']['checked_at'], pair,
                    {row['browser']:row['heartbeat'] for row in baseline['browsers']})
                if (not details.get('strict_termination_visible') or
                        'browser_killed_extension_stale' not in read('p43-diagnostics.json')['log']):
                    raise ValueError('Expected strict browser termination was not observed')
                for browser in pair:
                    statuses = [r['statuses'][browser] for r in read('p43-health-trace.json')]
                    if not any(r['state']=='inactive' and r.get('current_session_heartbeat') is False for r in statuses):
                        raise ValueError('Missing stopped-session health observation')
                    if not (directory / (browser+'-existing.png')).is_file():
                        raise ValueError('Visible strict-termination evidence missing')
            if scenario == 'outage':
                pages = observed['details']['pages']
                expected = [b+'-existing' for b in pair]
                expected += [b+'-new' for b in pair]
                if set(pages) != set(expected):
                    raise ValueError('Missing browser outage observation')
                for name,text in pages.items():
                    if not ('Blocked by BlocKuntu' in text and 'fail-closed' in text and
                            ('Daemon unavailable' in text or 'Daemon heartbeat missing' in text)):
                        raise ValueError('Visible outage page is missing')
                    if not (directory / (name+'.png')).is_file():
                        raise ValueError('Visible browser evidence missing')
            for browser in pair:
                if not (directory / (browser+'-baseline.png')).is_file() or not (directory / (browser+'-recovered.png')).is_file():
                    raise ValueError('Missing baseline/recovery browser screenshot')
                if 'BlocKuntu test route /free' not in read('p43-'+browser+'-baseline.json')['text']:
                    raise ValueError('Missing original allowed-tab baseline')
                text = read('p43-'+browser+'-recovered.json')['text']
                if not ('Blocked by BlocKuntu' in text and 'P41 exact '+browser in text and result['run_id']+'-'+browser in text):
                    raise ValueError('Missing recovered named block page for this run')
            recovery = read('p43-recovery.json')
            if not all(r.get('current_session_heartbeat') is True and r.get('state')=='active'
                       for r in recovery['heartbeats'].values()) or set(recovery['heartbeats']) != set(pair):
                raise ValueError('Both current-session recovery heartbeats required')
            browser_ids = recovery['browser_identities']
            if [r['browser'] for r in browser_ids] != list(pair) or any(not r['extension_unchanged'] for r in browser_ids):
                raise ValueError('Missing unchanged-profile extension identities')
            for browser in browser_ids:
                original = next(r for r in baseline['browsers'] if r['browser']==browser['browser'])
                if browser['profile'] != original['profile'] or browser['original_pid'] != original['main_pid']:
                    raise ValueError('Recovery replaced the original browser profile')
                if browser['recovered_pid']=='0' or ((browser['recovered_pid']==browser['original_pid']) != (scenario!='manifest')):
                    raise ValueError('Unexpected browser restart/preservation outcome')
            observations.append({'scenario': scenario, 'run_id': result['run_id'], 'required_passes': count,
                                 'clone_uuid': result['clone_uuid'], 'evidence': str(directory)})
        manifests.append(batch_manifest)
    if len(set(identities)) != 1:
        raise ValueError('Candidate changed between bundles')
    if qualify and len(set(extension_identities)) != 1:
        raise ValueError('Native browser version or published extension code changed between qualification bundles')
    if qualify and manifests[0] != manifests[1]:
        raise ValueError('Qualification requires identical captured harness versions')
    if qualify and len(preparation_modes) != 1:
        raise ValueError('Qualification requires comparable rolling fixture preparation')
    return {'status': 'pass', 'scope': guest_name + '-p43', 'adapter_qualified': qualify, 'batch_count': len(paths),
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
        result = {'status': 'fail', 'scope': 'native-p43', 'adapter_qualified': False, 'error': str(error)}
    save(args.output, result)
    print(result['status'], result.get('required_passes', ''), result.get('error', ''), args.output)
    return int(result['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
