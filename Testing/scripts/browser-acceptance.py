#!/usr/bin/env python3
"""Exact-package native P4.3 browser faults on independent guarded clones."""
import argparse
from layer4_platform import GUESTS, browsers_for, direct_removal, prerequisites
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shlex
import time

from layer4_browser_contract import CASES, SCENARIOS, finalize, redact, save, validate_action, verify_guest

spec = importlib.util.spec_from_file_location('browser_foundation', Path(__file__).with_name('uninstall-acceptance.py'))
foundation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(foundation)
vm, suite, ROOT = foundation.vm, foundation.suite, foundation.ROOT
checked, upload, REMOTE = foundation.checked, foundation.upload, foundation.REMOTE
SOURCE_PATHS = ('scripts/browser-acceptance.py', 'scripts/layer4_browser_contract.py',
    'scripts/layer4_uninstall_contract.py', 'scripts/layer4_platform.py', 'scripts/uninstall-acceptance.py',
    'scripts/vm-acceptance.py', 'scripts/phase0-vm.py', 'scripts/layer3_contract.py',
    'scripts/layer3_runtime.py', 'fixtures/layer3/guest.py', 'fixtures/layer4/uninstall_ops.py',
    'fixtures/layer4/uninstall.py', 'fixtures/layer3/accessibility.py', 'fixtures/layer3/ui_controls.py',
    'fixtures/layer3/browsers.py', 'fixtures/layer4/browser_ops.py',
    'fixtures/layer4/browser_recovery.py', 'fixtures/test-site/server.py')


def collect(record, destination, baseline=False):
    commands = {
        'guest-results.json': 'cat Testing/layer3/p43-result.json',
        'inspection.json': 'sudo -n /usr/bin/python3 Testing/layer3/browser_ops.py inspect',
        'collector.json': 'cat Testing/layer3/p43-collector.json',
        'collector-history.jsonl': 'cat Testing/layer3/p43-collector-history.jsonl',
        'journal.log': 'sudo -n journalctl -b --no-pager -u blockuntu.service -u blockuntu.socket -u blockuntu-watchdog.service',
        'collector.log': 'sudo -n journalctl -b --no-pager -u blockuntu-p43-collector.service -u blockuntu-p43-rescue.service',
        'suite.log': 'journalctl --user -b --no-pager -u blockuntu-p43-suite',
        'browser-journal.log': 'journalctl --user -b --no-pager ' + ' '.join('-u blockuntu-test-' + b for b in browsers_for(record['template'])),
        'event.log': 'sudo -n cat /etc/blockuntu/blockuntu.log',
    }
    if baseline:
        commands.update({'baseline.json': 'cat Testing/layer3/p43-baseline.json',
                         'export.json': 'cat Testing/layer3/p43-export.json',
                         'seeded.json': 'cat Testing/layer3/p43-seeded.json',
                         'extensions.json': 'cat Testing/layer3/p43-extensions.json'})
    errors = []
    for name, command in commands.items():
        try:
            answer = checked(record, command, check=False, timeout=45)
            if answer.returncode:
                raise RuntimeError('Evidence command returned ' + str(answer.returncode))
            if name.endswith('.json'):
                json.loads(answer.stdout)
            (destination / name).write_text(redact(answer.stdout))
            if answer.stderr:
                (destination / (name + '.stderr.log')).write_text(redact(answer.stderr))
        except Exception as error:
            errors.append({'file': name, 'error': redact(error)})
    for name in ('p43-plan.json','p43-fault.json','p43-observed.json','p43-restored.json','p43-recovery.json',
                 'p43-health-trace.json','p43-diagnostics.json','p43-rpc-trace.json',
                 *(f'p43-{b}-{s}.json' for b in browsers_for(record['template']) for s in ('baseline','existing','new','recovered','failure'))):
        try:
            answer = checked(record, 'cat Testing/layer3/' + name, check=False)
            if answer.returncode == 0:
                json.loads(answer.stdout)
                (destination / name).write_text(redact(answer.stdout))
        except Exception as error:
            errors.append({'file': name, 'error': redact(error)})
    return errors


def handle_action(record, destination, action, scenario):
    validate_action(action, scenario)
    vm.guarded(record)
    completion = {'id': action['id']}
    kind = action['action']
    if kind == 'fault':
        answer = checked(record, 'sudo -n /usr/bin/python3 Testing/layer3/browser_ops.py ' + action['name'], check=False, timeout=90)
        (destination / ('fault-' + action['name'] + '.log')).write_text(redact(answer.stdout + answer.stderr))
        if answer.returncode:
            raise RuntimeError('Fixed browser fault failed; see fault command log')
        completion.update(json.loads(answer.stdout))
    elif kind == 'checkpoint':
        point = destination / action['name']
        point.mkdir(mode=0o700, exist_ok=False)
        errors = collect(record, point, baseline=action['name'] == 'baseline')
        if errors:
            save(point / 'collection-errors.json', errors)
            raise RuntimeError('Incomplete browser checkpoint: ' + repr(errors))
    elif kind == 'screenshot':
        vm.virsh('screenshot', record['uuid'], destination / (action['name'] + '.png'))
    elif kind == 'keys':
        vm.virsh('send-key', record['uuid'], '--codeset','linux', *action['keys'])
    elif kind == 'type_url':
        for combination in [('KEY_LEFTCTRL','KEY_A'), *suite.url_keys(action['url']), ('KEY_ENTER',)]:
            vm.virsh('send-key', record['uuid'], '--codeset','linux','--holdtime','30',*combination)
            time.sleep(.025)
    checked(record, 'cat > Testing/layer3/host-action-done.json', input=json.dumps(completion))


def exercise(record, destination, scenario):
    checked(record, 'systemd-run --user --collect --unit=blockuntu-p43-suite --property=RuntimeMaxSec=1200 '
            '/usr/bin/python3 ' + REMOTE + 'browser_recovery.py --scenario ' + scenario + ' --run-id ' + record['run_id'] + ' --guest ' + record['template'])
    deadline, previous, last = time.monotonic() + 1240, None, time.monotonic()
    handled, reported = {}, set()
    history = destination / 'checkpoints'
    history.mkdir(mode=0o700)
    while time.monotonic() < deadline:
        answer = checked(record, 'cat Testing/layer3/host-action.json', check=False)
        if answer.returncode == 0:
            action = validate_action(json.loads(answer.stdout), scenario)
            if action['id'] in handled and handled[action['id']] != action:
                raise ValueError('Guest reused an action ID with different content')
            if action['id'] not in handled:
                handle_action(record, destination, action, scenario)
                handled[action['id']] = action
                save(destination / 'actions.json', list(handled.values()))
        answer = checked(record, 'cat Testing/layer3/p43-result.json', check=False)
        if answer.returncode == 0:
            guest = json.loads(answer.stdout)
            if guest != previous:
                save(history / (str(len(list(history.iterdir()))).zfill(4) + '.json'), guest)
                save(destination / 'guest-results.json', guest)
                previous = guest
            for row in guest.get('cases', []):
                if row['id'] not in reported:
                    print(record['run_id'], row['id'], row['status'], row.get('error', ''), flush=True)
                    reported.add(row['id'])
            if guest.get('status') in ('pass', 'fail'):
                return guest
        state = checked(record, 'systemctl --user is-active blockuntu-p43-suite', check=False).stdout.strip()
        if state not in ('active', 'activating'):
            answer = checked(record, 'cat Testing/layer3/p43-result.json', check=False)
            if answer.returncode == 0:
                guest = json.loads(answer.stdout)
                if guest.get('status') in ('pass', 'fail'):
                    return guest
            raise RuntimeError('Guest exited before a terminal browser checkpoint')
        if time.monotonic() - last > 30:
            print(record['run_id'], 'running; last cases:', sorted(reported), flush=True)
            last = time.monotonic()
        time.sleep(1)
    raise TimeoutError('Browser journey exceeded host monotonic deadline')


def run_scenario(run_id, scenario, package, report, guest_name='ubuntu', refresh_cachyos=False):
    record = vm.prepare(guest_name, run_id)
    destination = Path(record['evidence']) / 'acceptance'
    destination.mkdir(mode=0o700)
    captured = destination / 'harness'
    captured.mkdir(mode=0o700)
    manifest = {}
    for name in SOURCE_PATHS:
        source, target = ROOT / name, captured / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source.read_bytes())
        manifest[name] = hashlib.sha256(target.read_bytes()).hexdigest()
    save(destination / 'harness-files.json', manifest)
    save(destination / 'package-report.json', report)
    result = {'status': 'running', 'layer': 4, 'scope': guest_name + '-p43-' + scenario, 'guest': guest_name,
        'run_id': run_id, 'scenario': scenario, 'selected_case_ids': list(CASES[scenario]),
        'clone_uuid': record['uuid'], 'domain': record['domain'], 'package_commit': report['commit'],
        'package_filename': package.name, 'package_sha256': hashlib.sha256(package.read_bytes()).hexdigest(),
        'started_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'full_layer4_acceptance': False, 'clone_removed': False}
    result['refresh_cachyos'] = refresh_cachyos
    save(destination / 'result.json', result)
    started, guest, installed = time.monotonic(), None, False
    try:
        vm.verify(record)
        suite.boot(record)
        if guest_name == 'cachyos':
            if checked(record, 'pacman -Q blockuntu', check=False).returncode == 0:
                raise ValueError('Rolling fixture refresh requires BlocKuntu absent; app upgrade testing is deferred')
            vm.guarded(record)
            suite.prepare_cachyos(record, destination, refresh=refresh_cachyos)
            if checked(record, 'pacman -Q blockuntu', check=False).returncode == 0:
                raise ValueError('Unexpected BlocKuntu installation during OS fixture preparation')
        checked(record, 'mkdir -p Testing/layer3; chmod 700 Testing/layer3')
        for name in SOURCE_PATHS:
            source = ROOT / name
            if source.suffix == '.py' and source.name not in ('browser-acceptance.py', 'uninstall-acceptance.py', 'vm-acceptance.py', 'phase0-vm.py'):
                upload(record, source)
        checked(record, 'sudo -n /usr/bin/python3 Testing/layer3/browser_ops.py seed', timeout=360)
        upload(record, package, Path(suite.PLATFORMS[guest_name]['remote']).name)
        remote = suite.PLATFORMS[guest_name]['remote']
        if checked(record, 'sha256sum ' + remote).stdout.split()[0] != result['package_sha256']:
            raise ValueError('Transferred candidate checksum mismatch')
        answer = checked(record, suite.PLATFORMS[guest_name]['install'].format(package=shlex.quote(remote)), check=False, timeout=900)
        (destination / 'install.log').write_text(redact(answer.stdout + answer.stderr))
        if answer.returncode:
            raise RuntimeError('Exact candidate installation failed')
        version = checked(record, suite.PLATFORMS[guest_name]['version']).stdout.strip()
        if version != report['package_metadata']['version']:
            raise ValueError('Installed version differs from candidate report')
        result['installed_version'], installed = version, True
        checked(record, 'sudo -n usermod -aG blockuntu akhi')
        if guest_name == 'cachyos':
            suite.prepare_cachyos_keyboard(record, destination)
        old_boot = checked(record, 'cat /proc/sys/kernel/random/boot_id').stdout.strip()
        checked(record, 'sudo -n systemctl reboot', check=False)
        time.sleep(5)
        def rebooted():
            record['ip'] = vm.address(record) or record['ip']
            answer = checked(record, 'cat /proc/sys/kernel/random/boot_id', check=False)
            return answer.returncode == 0 and answer.stdout.strip() != old_boot
        vm.wait_until(rebooted, 'candidate/group reboot')
        vm.graphical_check(record, '-p43-installed')
        if 'blockuntu' not in checked(record, 'id -nG').stdout.split():
            raise ValueError('Group membership not refreshed')
        answer = checked(record, prerequisites(guest_name, 'browser'), timeout=600, check=False)
        (destination / 'prerequisites.log').write_text(redact(answer.stdout + answer.stderr))
        if answer.returncode:
            raise RuntimeError('Accessibility prerequisite installation failed')
        checked(record, 'sudo -n systemd-run --collect --unit=blockuntu-p43-collector --property=RuntimeMaxSec=1500 '
                '/usr/bin/python3 ' + REMOTE + 'browser_ops.py collect')
        vm.virsh('screenshot', record['uuid'], destination / 'p43-console-preflight.png')
        result['console_access_verified'] = True
        answer = checked(record, 'cat /etc/os-release; uname -r; id; loginctl list-sessions --no-legend')
        (destination / 'environment.txt').write_text(redact(answer.stdout))
        guest = exercise(record, destination, scenario)
        result['required_passes'] = verify_guest(guest, scenario, guest_name)
        result['status'] = 'pass'
    except Exception as error:
        result.update(status='fail', error=redact(error))
    finally:
        if installed:
            try:
                checked(record, 'sudo -n /usr/bin/python3 Testing/layer3/browser_ops.py restore', timeout=45)
            except Exception as error:
                result.update(status='fail', recovery_error=redact(error))
        if installed:
            result['collection_errors'] = collect(record, destination, baseline=bool(guest and guest.get('cases', [{}])[0].get('status') == 'pass'))
            if result['collection_errors']:
                result['status'] = 'fail'
        finalize(vm, suite, record, result)
        result['finished_at'] = dt.datetime.now(dt.timezone.utc).isoformat()
        result['host_elapsed_seconds'] = round(time.monotonic() - started, 3)
        save(destination / 'result.json', result)
    print(run_id, result['status'], destination, result.get('error', ''), flush=True)
    return result, destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--guest', choices=GUESTS, default='ubuntu')
    parser.add_argument('--refresh-cachyos', action='store_true',
                        help='Refresh only the disposable rolling OS before installing BlocKuntu')
    parser.add_argument('--package-report', type=Path, required=True)
    parser.add_argument('--scenario', choices=('all', *SCENARIOS), default='all')
    args = parser.parse_args()
    if args.refresh_cachyos and args.guest != 'cachyos':
        parser.error('--refresh-cachyos requires --guest cachyos')
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,27}', args.run_id):
        parser.error('Batch ID requires 1–28 lowercase letters, digits or hyphens')
    package, report = suite.artifact(args.package_report, args.guest)
    guest_name = args.guest
    selected = SCENARIOS if args.scenario == 'all' else (args.scenario,)
    vm.RUNTIME.mkdir(parents=True, exist_ok=True)
    with (vm.RUNTIME / 'runner.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        destination = ROOT / 'results' / args.run_id
        if destination.exists() or any((vm.RUNTIME / (args.run_id + '-' + s)).exists() or (ROOT / 'results' / (args.run_id + '-' + s)).exists() for s in selected):
            raise ValueError('Every batch/clone ID must be fresh')
        vm.base_records()
        vm.guestfs_setup()
        destination.mkdir(mode=0o700)
        summary = {'status': 'running', 'layer': 4, 'scope': guest_name + '-p43', 'guest': guest_name,
            'selected_scenarios': list(selected), 'refresh_cachyos': args.refresh_cachyos, 'package_commit': report['commit'],
            'package_sha256': hashlib.sha256(package.read_bytes()).hexdigest(), 'runs': []}
        save(destination / 'summary.json', summary)
        for scenario in selected:
            try:
                result, evidence = run_scenario(args.run_id + '-' + scenario, scenario, package, report, guest_name, args.refresh_cachyos)
            except Exception as error:
                summary.update(status='fail', preparation_error=redact(error), failed_scenario=scenario)
                save(destination / 'summary.json', summary)
                raise
            summary['runs'].append({'scenario': scenario, 'status': result['status'], 'evidence': str(evidence),
                'base_integrity_verified': result.get('base_integrity_verified', False),
                'clone_removed': result.get('clone_removed', False), 'clone_retained': result.get('clone_retained', False)})
            save(destination / 'summary.json', summary)
            if result.get('safety_error'):
                break
        summary['status'] = 'pass' if len(summary['runs']) == len(selected) and all(r['status'] == 'pass' for r in summary['runs']) else 'fail'
        save(destination / 'summary.json', summary)
    return int(summary['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
