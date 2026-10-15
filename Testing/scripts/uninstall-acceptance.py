#!/usr/bin/env python3
"""Small P4.0 foundation and exact-package native P4.1 on guarded clones."""
import argparse
from layer4_platform import GUESTS, browsers_for, direct_removal, prerequisites, refusal_message
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shlex
import time

from layer4_uninstall_contract import (SCENARIOS, CASES, cases_for, finalize, input_keys,
                                       redact, save, validate_action, verify_guest)

spec = importlib.util.spec_from_file_location('p41_acceptance', Path(__file__).with_name('vm-acceptance.py'))
suite = importlib.util.module_from_spec(spec)
spec.loader.exec_module(suite)
vm = suite.vm
ROOT = vm.ROOT
REMOTE = '/home/akhi/Testing/layer3/'
SOURCES = [Path(__file__), ROOT / 'scripts/layer4_uninstall_contract.py', ROOT / 'scripts/layer4_platform.py',
           ROOT / 'scripts/vm-acceptance.py', ROOT / 'scripts/phase0-vm.py',
           ROOT / 'scripts/layer3_contract.py', ROOT / 'scripts/layer3_runtime.py',
           *[ROOT / 'fixtures/layer3' / name for name in ('guest.py', 'accessibility.py',
                                                        'ui_controls.py', 'browsers.py')],
           ROOT / 'fixtures/layer4/uninstall.py', ROOT / 'fixtures/layer4/uninstall_ops.py',
           ROOT / 'fixtures/test-site/server.py', ROOT / 'fixtures/test-processes/test_process.c',
           ROOT / 'scripts/build-test-processes.sh']


def checked(record, command, **kwargs):
    """Revalidate clone ownership/storage before each guest action."""
    vm.guarded(record)
    return vm.ssh(record, command, **kwargs)


def upload(record, source, name=None):
    vm.guarded(record)
    suite.upload(record, source, REMOTE + (name or Path(source).name))


def collect(record, destination, scenario, baseline=False):
    errors = []
    commands = {
        'guest-results.json': 'cat Testing/layer3/p41-result.json',
        'inspection.json': 'sudo -n /usr/bin/python3 Testing/layer3/uninstall_ops.py inspect',
        'collector.json': 'cat Testing/layer3/p41-collector.json',
        'journal.log': 'sudo -n journalctl -b --no-pager -u blockuntu.service -u blockuntu.socket -u blockuntu-watchdog.service -u blockuntu-hosts.service',
        'collector.log': 'sudo -n journalctl -b --no-pager -u blockuntu-p41-collector.service',
        'suite.log': 'journalctl --user -b --no-pager -u blockuntu-p41-suite',
        'gui.log': 'journalctl --user -b --no-pager -u blockuntu-acceptance-gui',
        ('dpkg.log' if record['template'] == 'ubuntu' else 'native-package.log'): 'sudo -n /usr/bin/python3 Testing/layer3/uninstall_ops.py native-log',
    }
    if baseline:
        commands.update({'baseline.json': 'cat Testing/layer3/p41-baseline.json',
                         'export.json': 'cat Testing/layer3/p41-export.json',
                         'seeded.json': 'cat Testing/layer3/p41-seeded.json'})
    for name, command in commands.items():
        try:
            answer = checked(record, command, check=False, timeout=45)
            if answer.returncode:
                raise RuntimeError('Evidence command returned ' + str(answer.returncode))
            (destination / name).write_text(redact(answer.stdout + answer.stderr))
        except Exception as error:
            errors.append({'file': name, 'error': redact(error)})
    for name in ('p41-gui-baseline.json', 'p41-gui-post-refusal.json',
                 'p41-before-uninstall.json', 'p41-after.json', 'p41-cleanup.json',
                 'p41-handoff.json', 'p41-gui-failure.json', 'p41-firefox-failure.json',
                 'p41-' + browsers_for(record['template'])[1] + '-failure.json', 'p41-cleanup-findings.json', 'p41-browser-recovery.json'):
        try:
            answer = checked(record, 'cat Testing/layer3/' + name, check=False)
            if answer.returncode == 0:
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
        # Name/scenario was validated. No guest path or command is interpolated.
        answer = checked(record, 'sudo -n env -u BLOCKUNTU_PACKAGE_REMOVAL_LEASE ' +
                         direct_removal(record['template'], scenario), check=False, timeout=90)
        (destination / ('direct-' + scenario + '.log')).write_text(redact(answer.stdout + answer.stderr))
        completion.update(returncode=answer.returncode,
                          refusal_observed=refusal_message(record['template']) in answer.stdout + answer.stderr)
    elif kind == 'checkpoint':
        point = destination / action['name']
        point.mkdir(exist_ok=False, mode=0o700)
        errors = collect(record, point, scenario, baseline=action['name'] == 'baseline')
        if errors:
            save(point / 'collection-errors.json', errors)
            raise RuntimeError('Incomplete pre/post fault checkpoint: ' + repr(errors))
    elif kind == 'screenshot':
        vm.virsh('screenshot', record['uuid'], destination / (action['name'] + '.png'))
    elif kind == 'keys':
        vm.virsh('send-key', record['uuid'], '--codeset', 'linux', *action['keys'])
    elif kind == 'focus_gui':
        # The authorized baseline has three applications. Keeping Alt held
        # while moving one place past Alt-Tab reaches the third application;
        # independent Alt-Tab chords would only toggle the two browsers.
        # The guest must observe the actual GUI's active frame before input.
        vm.virsh('send-key', record['uuid'], '--codeset', 'linux', '--holdtime', '100',
                 'KEY_LEFTALT', 'KEY_TAB', 'KEY_RIGHT')
    else:
        credential = kind == 'type_credential'
        if credential:
            value = ('BLOCKUNTU-UNINSTALL-RECOVERY-INVALID' if action['slot'] == 'invalid' else
                     checked(record, 'cat /etc/blockuntu/uninstall-recovery.txt').stdout.strip())
            keys = input_keys(value)
            completion['typed_characters'] = len(value)
            del value
        else:
            url = action['url']
            if url in ('about:addons', 'chrome://extensions'):
                mapping = {':': ('KEY_LEFTSHIFT', 'KEY_SEMICOLON'), '/': ('KEY_SLASH',)}
                keys = [mapping[c] if c in mapping else ('KEY_' + c.upper(),) for c in url]
            else:
                keys = suite.url_keys(url)
        for combination in [('KEY_LEFTCTRL', 'KEY_A'), *keys, ('KEY_TAB',) if credential else ('KEY_ENTER',)]:
            vm.virsh('send-key', record['uuid'], '--codeset', 'linux', '--holdtime', '30', *combination)
            time.sleep(0.08 if credential else 0.025)
    checked(record, 'cat > Testing/layer3/host-action-done.json', input=json.dumps(completion))


def exercise(record, destination, scenario):
    checked(record, 'systemd-run --user --collect --unit=blockuntu-p41-suite '
            '--property=RuntimeMaxSec=1200 /usr/bin/python3 ' + REMOTE + 'uninstall.py '
            '--scenario ' + scenario + ' --run-id ' + record['run_id'] + ' --guest ' + record['template'])
    deadline = time.monotonic() + 1240
    handled, reported = {}, set()
    history = destination / 'checkpoints'
    history.mkdir(mode=0o700)
    previous = None
    last = time.monotonic()
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
        answer = checked(record, 'cat Testing/layer3/p41-result.json', check=False)
        if answer.returncode == 0:
            result = json.loads(answer.stdout)
            if result != previous:
                save(history / (str(len(list(history.iterdir()))).zfill(4) + '.json'), result)
                save(destination / 'guest-results.json', result)
                previous = result
            for row in result.get('cases', []):
                if row['id'] not in reported:
                    print(record['run_id'], row['id'], row['status'], row.get('error', ''), flush=True)
                    reported.add(row['id'])
            if result.get('status') in ('pass', 'fail'):
                return result
        state = checked(record, 'systemctl --user is-active blockuntu-p41-suite', check=False).stdout.strip()
        if state not in ('active', 'activating'):
            # Reread after exit: terminal publication can race the preceding read.
            answer = checked(record, 'cat Testing/layer3/p41-result.json', check=False)
            if answer.returncode == 0:
                result = json.loads(answer.stdout)
                if result.get('status') in ('pass', 'fail'):
                    return result
            raise RuntimeError('Guest exited before a terminal checkpoint')
        if time.monotonic() - last > 30:
            print(record['run_id'], 'running; last cases:', sorted(reported), flush=True)
            last = time.monotonic()
        time.sleep(1)
    raise TimeoutError('Uninstall journey exceeded its host monotonic deadline')


def run_scenario(run_id, scenario, package, report, guest_name='ubuntu', refresh_cachyos=False):
    record = vm.prepare(guest_name, run_id)
    destination = Path(record['evidence']) / 'acceptance'
    destination.mkdir(mode=0o700)
    captured = destination / 'harness'
    captured.mkdir(mode=0o700)
    manifest = {}
    for source in SOURCES:
        relative = source.relative_to(ROOT)
        target = captured / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source.read_bytes())
        manifest[str(relative)] = hashlib.sha256(target.read_bytes()).hexdigest()
    save(destination / 'harness-files.json', manifest)
    save(destination / 'package-report.json', report)
    result = {'status': 'running', 'layer': 4, 'scope': guest_name + '-p41-' + scenario,
              'run_id': run_id, 'scenario': scenario, 'guest': guest_name,
              'selected_case_ids': list(cases_for(guest_name)[scenario]), 'clone_uuid': record['uuid'],
              'domain': record['domain'], 'package_commit': report['commit'],
              'package_filename': package.name, 'package_sha256': hashlib.sha256(package.read_bytes()).hexdigest(),
              'started_at': dt.datetime.now(dt.timezone.utc).isoformat(),
              'full_layer4_acceptance': False, 'clone_removed': False,
              'polkit_fixture': 'clone-only active local akhi; exact leased native GUI removal argv'}
    result['refresh_cachyos'] = refresh_cachyos
    save(destination / 'result.json', result)
    started = time.monotonic()
    guest = None
    installed = False
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
        for source in SOURCES:
            if source.suffix == '.py' and source.name not in ('uninstall-acceptance.py', 'phase0-vm.py', 'vm-acceptance.py'):
                upload(record, source)
        upload(record, ROOT / 'fixtures/test-site/server.py')
        for name in ('blockuntu-test-block', 'blockuntu-test-app-b'):
            upload(record, ROOT / 'artifacts/fixtures/bin' / name, name.replace('blockuntu-test-', 'bk-test-'))
        checked(record, 'chmod +x Testing/layer3/bk-test-block Testing/layer3/bk-test-app-b')
        checked(record, 'sudo -n /usr/bin/python3 Testing/layer3/uninstall_ops.py seed', timeout=360)
        upload(record, package, Path(suite.PLATFORMS[guest_name]['remote']).name)
        remote = suite.PLATFORMS[guest_name]['remote']
        if checked(record, 'sha256sum ' + remote).stdout.split()[0] != result['package_sha256']:
            raise ValueError('Transferred package checksum mismatch')
        answer = checked(record, suite.PLATFORMS[guest_name]['install'].format(package=shlex.quote(remote)), check=False, timeout=900)
        (destination / 'install.log').write_text(redact(answer.stdout + answer.stderr))
        if answer.returncode:
            raise RuntimeError('Exact candidate installation failed')
        version = checked(record, suite.PLATFORMS[guest_name]['version']).stdout.strip()
        if version != report['package_metadata']['version']:
            raise ValueError('Installed version does not match candidate report')
        result['installed_version'] = version
        installed = True
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
        vm.wait_until(rebooted, 'candidate install/group reboot')
        vm.graphical_check(record, '-p41-installed')
        if 'blockuntu' not in checked(record, 'id -nG').stdout.split():
            raise ValueError('Group membership not refreshed')
        if scenario == 'authorized' or guest_name != 'ubuntu':
            answer = checked(record, prerequisites(guest_name, 'browser'), timeout=600, check=False)
            (destination / 'prerequisites.log').write_text(redact(answer.stdout + answer.stderr))
            if answer.returncode:
                raise RuntimeError('Accessibility prerequisite installation failed')
        checked(record, 'sudo -n systemd-run --collect --unit=blockuntu-p41-collector '
                '--property=RuntimeMaxSec=1500 /usr/bin/python3 ' + REMOTE + 'uninstall_ops.py collect')
        vm.virsh('screenshot', record['uuid'], destination / 'p41-console-preflight.png')
        # This transport and root unit use no BlocKuntu binary/socket/credential.
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
            result['collection_errors'] = collect(record, destination, scenario,
                baseline=bool(guest and guest.get('cases', [{}])[0].get('status') == 'pass'))
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
        parser.error('Batch run ID requires 1–28 lowercase letters, digits or hyphens')
    package, report = suite.artifact(args.package_report, args.guest)
    guest_name = args.guest
    selected = SCENARIOS if args.scenario == 'all' else (args.scenario,)
    vm.RUNTIME.mkdir(parents=True, exist_ok=True)
    with (vm.RUNTIME / 'runner.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        destination = ROOT / 'results' / args.run_id
        if destination.exists() or any((vm.RUNTIME / (args.run_id + '-' + s)).exists() or
                                      (ROOT / 'results' / (args.run_id + '-' + s)).exists() for s in selected):
            raise ValueError('Every batch/clone run ID must be fresh')
        vm.base_records()
        vm.guestfs_setup()
        vm.run(['bash', ROOT / 'scripts/build-test-processes.sh'])
        destination.mkdir(mode=0o700)
        summary = {'status': 'running', 'layer': 4, 'guest': guest_name,
                   'selected_scenarios': list(selected), 'refresh_cachyos': args.refresh_cachyos, 'package_commit': report['commit'],
                   'package_sha256': hashlib.sha256(package.read_bytes()).hexdigest(), 'runs': []}
        save(destination / 'summary.json', summary)
        for scenario in selected:
            try:
                result, evidence = run_scenario(args.run_id + '-' + scenario, scenario, package, report, guest_name, args.refresh_cachyos)
            except Exception as error:
                # Preparation can fail before a suite result exists. Preserve
                # that failure rather than leaving a misleading running batch.
                summary.update(status='fail', preparation_error=redact(error), failed_scenario=scenario)
                save(destination / 'summary.json', summary)
                raise
            summary['runs'].append({'scenario': scenario, 'status': result['status'], 'evidence': str(evidence),
                                    'base_integrity_verified': result.get('base_integrity_verified', False),
                                    'clone_removed': result.get('clone_removed', False),
                                    'clone_retained': result.get('clone_retained', False)})
            save(destination / 'summary.json', summary)
            if result.get('safety_error'):
                break
        summary['status'] = 'pass' if len(summary['runs']) == len(selected) and all(r['status'] == 'pass' for r in summary['runs']) else 'fail'
        save(destination / 'summary.json', summary)
    return int(summary['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
