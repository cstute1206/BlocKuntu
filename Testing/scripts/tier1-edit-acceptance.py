#!/usr/bin/env python3
"""Focused installed-package Tier 1 editing; no browser-store dependency."""
import argparse
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shlex
import time

from protected_edit_contract import CASE_IDS, input_keys, validate_action, verify_guest

spec = importlib.util.spec_from_file_location('acceptance', Path(__file__).with_name('vm-acceptance.py'))
suite = importlib.util.module_from_spec(spec)
spec.loader.exec_module(suite)
vm = suite.vm
ROOT = vm.ROOT


def exercise(record, destination):
    sources = [ROOT / 'fixtures/layer3' / name for name in
               ('guest.py', 'accessibility.py', 'ui_controls.py')]
    sources += [ROOT / 'scripts' / name for name in ('layer3_contract.py', 'layer3_runtime.py')]
    sources += [ROOT / 'fixtures/layer4/protected_edits.py']
    for source in sources:
        suite.upload(record, source, '/home/akhi/Testing/layer3/' + source.name)
    inhibitor = 'kde-inhibit --power --screenSaver ' if record['template'] == 'cachyos' else ''
    vm.ssh(record, 'systemd-run --user --collect --unit=blockuntu-protected-edits '
           '--property=RuntimeMaxSec=720 ' + inhibitor + '/usr/bin/python3 /home/akhi/Testing/layer3/protected_edits.py --guest ' + record['template'])
    deadline = time.monotonic() + 750
    handled = {}
    reported = set()
    while time.monotonic() < deadline:
        answer = vm.ssh(record, 'cat Testing/layer3/host-action.json 2>/dev/null', check=False)
        if answer.stdout:
            action = validate_action(json.loads(answer.stdout))
            if action['id'] in handled and action != handled[action['id']]:
                raise ValueError('Reused action ID with different content')
            if action['id'] not in handled:
                vm.guarded(record)
                completion = {'id': action['id']}
                if action['action'] == 'screenshot':
                    vm.virsh('screenshot', record['uuid'], destination / (action['name'] + '.png'))
                elif action['action'] == 'keys':
                    vm.virsh('send-key', record['uuid'], '--codeset', 'linux', *action['keys'])
                else:
                    if action['action'] == 'type_form':
                        value = action['value']
                    elif action['slot'] == 'invalid':
                        value = 'BLOCKUNTU-TIER1-EDIT-INVALID'
                    else:
                        # Deliberately no log argument: never persist the key.
                        value = vm.ssh(record, 'cat /etc/blockuntu/tier1-edit-key.txt').stdout.strip()
                    credential = action['action'] == 'type_credential'
                    if credential:
                        completion['typed_characters'] = len(value)
                    for keys in [('KEY_LEFTCTRL', 'KEY_A'), *input_keys(value), ('KEY_TAB',)]:
                        vm.virsh('send-key', record['uuid'], '--codeset', 'linux', '--holdtime', '40' if credential else '20', *keys)
                        time.sleep(0.12 if credential else 0.04)
                    del value
                handled[action['id']] = action
                vm.ssh(record, 'cat > Testing/layer3/host-action-done.json', input=json.dumps(completion))
        answer = vm.ssh(record, 'cat Testing/layer3/protected-result.json 2>/dev/null', check=False)
        if answer.stdout:
            result = json.loads(answer.stdout)
            vm.save(destination / 'guest-results.json', result)
            for row in result['cases']:
                if row['id'] not in reported:
                    print(row['id'], row['status'], row.get('error', ''), flush=True)
                    reported.add(row['id'])
            if result['status'] != 'running':
                return result
        state = vm.ssh(record, 'systemctl --user is-active blockuntu-protected-edits', check=False).stdout.strip()
        if state not in ('active', 'activating'):
            # The guest atomically publishes its terminal checkpoint before
            # exiting. It may finish between our earlier read and this service
            # query; reread after exit before treating it as interrupted.
            answer = vm.ssh(record, 'cat Testing/layer3/protected-result.json 2>/dev/null', check=False)
            if answer.returncode == 0 and answer.stdout:
                result = json.loads(answer.stdout)
                vm.save(destination / 'guest-results.json', result)
                if result.get('status') in ('pass', 'fail'):
                    return result
            raise RuntimeError('Guest journey exited before a terminal checkpoint')
        time.sleep(1)
    raise TimeoutError('Protected-edit journey exceeded 750 seconds')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--guest', choices=sorted(suite.PLATFORMS), default='ubuntu')
    parser.add_argument('--package-report', type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,39}', args.run_id):
        parser.error('Run ID requires 1-40 lowercase letters, digits or hyphens')
    selected_guest = args.guest
    platform = suite.PLATFORMS[selected_guest]
    package, report = suite.artifact(args.package_report, selected_guest)
    vm.RUNTIME.mkdir(parents=True, exist_ok=True)
    with (vm.RUNTIME / 'runner.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        vm.guestfs_setup()
        record = vm.prepare(selected_guest, args.run_id)
        destination = Path(record['evidence']) / 'acceptance'
        destination.mkdir(mode=0o700)
        sources = [Path(__file__), ROOT / 'scripts/protected_edit_contract.py',
                   ROOT / 'scripts/vm-acceptance.py', ROOT / 'scripts/phase0-vm.py',
                   ROOT / 'scripts/layer3_contract.py', ROOT / 'scripts/layer3_runtime.py',
                   ROOT / 'fixtures/layer4/protected_edits.py',
                   *[ROOT / 'fixtures/layer3' / name for name in ('guest.py', 'accessibility.py', 'ui_controls.py')]]
        vm.save(destination / 'harness-files.json', {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources})
        vm.save(destination / 'package-report.json', report)
        result = {'status': 'running', 'scope': selected_guest + '-tier1-protected-edits', 'layer': 4,
                  'run_id': args.run_id, 'clone_uuid': record['uuid'], 'guest': selected_guest,
                  'selected_case_ids': list(CASE_IDS), 'package_commit': report['commit'],
                  'package_sha256': hashlib.sha256(package.read_bytes()).hexdigest(),
                  'started_at': dt.datetime.now(dt.timezone.utc).isoformat(),
                  'full_layer3_acceptance': False, 'full_layer4_acceptance': False}
        vm.save(destination / 'result.json', result)
        try:
            vm.verify(record)
            suite.boot(record)
            if selected_guest == 'cachyos':
                suite.prepare_cachyos(record, destination)
            vm.ssh(record, 'mkdir -p Testing/layer3; chmod 700 Testing/layer3')
            remote = platform['remote']
            suite.upload(record, package, remote)
            assert vm.ssh(record, 'sha256sum ' + shlex.quote(remote)).stdout.split()[0] == result['package_sha256']
            installed = vm.ssh(record, platform['install'].format(package=shlex.quote(remote)), timeout=900, check=False)
            (destination / 'install.log').write_text(installed.stdout + installed.stderr)
            if installed.returncode:
                raise RuntimeError('Exact package installation failed')
            version = vm.ssh(record, platform['version']).stdout.strip()
            assert version == report['package_metadata']['version']
            result['installed_version'] = version
            vm.ssh(record, 'sudo -n usermod -aG blockuntu akhi')
            if selected_guest == 'cachyos':
                vm.ssh(record, 'kwriteconfig6 --file kxkbrc --group Layout --key Use true')
                vm.ssh(record, 'kwriteconfig6 --file kxkbrc --group Layout --key LayoutList us')
            old_boot = vm.ssh(record, 'cat /proc/sys/kernel/random/boot_id').stdout.strip()
            vm.ssh(record, 'sudo -n systemctl reboot', check=False)
            time.sleep(5)
            def rebooted():
                record['ip'] = vm.address(record) or record['ip']
                answer = vm.ssh(record, 'cat /proc/sys/kernel/random/boot_id', check=False)
                return answer.returncode == 0 and answer.stdout.strip() != old_boot
            vm.wait_until(rebooted, 'installed package reboot')
            vm.graphical_check(record, '-installed')
            assert 'blockuntu' in vm.ssh(record, 'id -nG').stdout.split()
            prerequisites = {
                'ubuntu': 'sudo -n apt-get update && sudo -n env DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=180 install -y python3-pyatspi',
                'fedora': 'sudo -n dnf install -y python3-pyatspi',
                'cachyos': 'sudo -n pacman -S --noconfirm --needed python-atspi',
            }
            for name, command in {
                    'environment.txt': 'cat /etc/os-release; uname -r; id',
                    'prerequisites.log': prerequisites[selected_guest]}.items():
                answer = vm.ssh(record, command, timeout=600, check=False)
                (destination / name).write_text(answer.stdout + answer.stderr)
                if answer.returncode:
                    raise RuntimeError('Setup failed: ' + name)
            guest = exercise(record, destination)
            result['required_passes'] = verify_guest(guest, expected_guest=selected_guest)
            result['status'] = 'pass'
        except Exception as error:
            result.update(status='fail', error=str(error))
        finally:
            for name, command in {
                    'suite.log': 'journalctl --user -b --no-pager -u blockuntu-protected-edits',
                    'journal.log': 'sudo -n journalctl -b --no-pager -u blockuntu.service',
                    'protected-failure.json': 'cat Testing/layer3/protected-failure.json',
                    'guest-results.json': 'cat Testing/layer3/protected-result.json'}.items():
                try:
                    answer = vm.ssh(record, command, check=False)
                    if answer.returncode == 0:
                        (destination / name).write_text(answer.stdout + answer.stderr)
                except Exception:
                    pass
            try:
                suite.stop(record)
                vm.cleanup(record)
                result.update(base_integrity_verified=True, clone_removed=True)
            except Exception as error:
                result.update(status='fail', cleanup_error=str(error))
            result['finished_at'] = dt.datetime.now(dt.timezone.utc).isoformat()
            vm.save(destination / 'result.json', result)
        print(result['status'], destination, result.get('error', ''), flush=True)
        return int(result['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
