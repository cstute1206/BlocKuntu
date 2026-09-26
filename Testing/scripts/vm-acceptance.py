#!/usr/bin/env python3
"""Run installed-package acceptance on an owned disposable libvirt clone."""
from __future__ import annotations

import argparse
import base64
import datetime as dt
import fcntl
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shlex
import sys
import time

SPEC = importlib.util.spec_from_file_location('phase0_vm', Path(__file__).with_name('phase0-vm.py'))
vm = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(vm)
ROOT = vm.ROOT

PLATFORMS = {
    'ubuntu': {
        'component': 'deb',
        'suffix': '.deb',
        'remote': '/home/akhi/Testing/layer3/blockuntu.deb',
        'install': ('sudo -n env DEBIAN_FRONTEND=noninteractive apt-get '
                    '-o DPkg::Lock::Timeout=180 install -y {package}'),
        'version': "dpkg-query -W -f='${Version}' blockuntu",
    },
    'fedora': {
        'component': 'rpm',
        'suffix': '.rpm',
        'remote': '/home/akhi/Testing/layer3/blockuntu.rpm',
        'install': 'sudo -n dnf install -y {package}',
        'version': "rpm -q --qf '%{VERSION}-%{RELEASE}' blockuntu",
    },
    'cachyos': {
        'component': 'arch',
        'suffix': '.pkg.tar.zst',
        'remote': '/home/akhi/Testing/layer3/blockuntu.pkg.tar.zst',
        'install': 'sudo -n pacman --noconfirm -U {package}',
        'version': "pacman -Q blockuntu | awk '{print $2}'",
    },
}


def artifact(report_path, guest='ubuntu'):
    if guest not in PLATFORMS:
        raise ValueError(f'Unsupported acceptance guest: {guest}')
    platform = PLATFORMS[guest]
    report_path = Path(report_path).resolve()
    report = json.loads(report_path.read_text())
    if report.get('passed') is not True or report.get('build_provenance_verified') is not True:
        raise ValueError('Acceptance requires a passed, provenance-verified Layer 2 report')
    if (report.get('component') != platform['component'] or
            not re.fullmatch(r'[0-9a-f]{40}', report.get('commit', ''))):
        raise ValueError(f"{guest} acceptance requires a {platform['component']} report and full source commit")
    entries = [a for a in report['artifacts'] if a['filename'].endswith(platform['suffix'])]
    if len(entries) != 1:
        raise ValueError(f"Expected exactly one {platform['suffix']} artifact")
    entry = entries[0]
    if Path(entry['filename']).name != entry['filename']:
        raise ValueError('Artifact filename must be a basename')
    package = report_path.parent / 'artifacts' / entry['filename']
    if package.is_symlink() or hashlib.sha256(package.read_bytes()).hexdigest() != entry['sha256']:
        raise ValueError('Package SHA-256 differs from the Layer 2 report')
    return package, report


def upload(record, source, destination):
    vm.ssh(record, 'base64 -d > ' + shlex.quote(destination),
           input=base64.b64encode(Path(source).read_bytes()).decode(), timeout=120)


def url_keys(url):
    """Encode only fixture/policy URLs for native browser address-bar input."""
    if not isinstance(url, str) or len(url) > 160 or not (
        url in ('about:policies', 'chrome://policy') or
        re.fullmatch(r'http://(?:web|allowed|outside)\.blockuntu\.test:18080/[a-z/]+', url)
    ):
        raise ValueError('Guest requested a URL outside the acceptance fixture')
    punctuation = {'.': ('KEY_DOT',), '/': ('KEY_SLASH',),
                   ':': ('KEY_LEFTSHIFT', 'KEY_SEMICOLON'), '-': ('KEY_MINUS',)}
    return [punctuation[c] if c in punctuation else ('KEY_' + c.upper(),) for c in url]


def boot(record):
    vm.guarded(record)
    vm.virsh('start', record['uuid'])
    record['ip'] = vm.wait_for_address(record)
    vm.wait_until(lambda: vm.ssh(record, 'true', check=False).returncode == 0, 'SSH')
    vm.save(vm.RUNTIME / record['run_id'] / record['template'] / 'owner.json', record)


def stop(record):
    vm.guarded(record)
    if vm.virsh('domstate', record['uuid']).stdout.strip() != 'shut off':
        vm.virsh('shutdown', record['uuid'])
        try:
            vm.wait_until(lambda: vm.virsh('domstate', record['uuid']).stdout.strip() == 'shut off',
                          'shutdown', seconds=60)
        except TimeoutError:
            vm.guarded(record)
            vm.virsh('destroy', record['uuid'])
    vm.unchanged(record)


def prepare_cachyos(record, destination, refresh=False):
    """Record the supplied baseline; refresh only when explicitly requested."""
    baseline = vm.ssh(record, 'cat /etc/os-release; uname -r; df -h /; lsblk -b -o NAME,SIZE,FSTYPE,MOUNTPOINTS; pacman -Q', check=False)
    (destination / 'cachyos-baseline.log').write_text(baseline.stdout + baseline.stderr)
    if baseline.returncode:
        raise RuntimeError('Could not inspect CachyOS baseline')
    if not refresh:
        return
    def clean_cache(stage):
        name = f'pacman-cache-{stage}.log'
        cleaned = vm.ssh(record, "printf 'y\\ny\\n' | sudo -n env LC_ALL=C pacman -Scc",
                         timeout=120, check=False)
        (destination / name).write_text(cleaned.stdout + cleaned.stderr)
        if cleaned.returncode:
            raise RuntimeError(f'CachyOS package cache cleanup failed; see {name}')
    clean_cache('before-update')
    updated = vm.ssh(record, 'sudo -n pacman -Syyu --noconfirm', timeout=1800, check=False)
    (destination / 'os-update.log').write_text(updated.stdout + updated.stderr)
    if updated.returncode:
        raise RuntimeError('CachyOS system update failed; see os-update.log')
    clean_cache('after-update')
    boot_id = vm.ssh(record, 'cat /proc/sys/kernel/random/boot_id').stdout.strip()
    vm.ssh(record, 'sudo -n systemctl reboot', check=False)
    time.sleep(5)
    def rebooted():
        record['ip'] = vm.address(record) or record['ip']
        answer = vm.ssh(record, 'cat /proc/sys/kernel/random/boot_id', check=False)
        return answer.returncode == 0 and answer.stdout.strip() != boot_id
    vm.wait_until(rebooted, 'post-update CachyOS reboot')
    vm.graphical_check(record, '-updated')


def collect(record, destination):
    for name, command in {
        'journal.log': 'sudo -n journalctl -b --no-pager -u blockuntu.service -u blockuntu.socket -u blockuntu-watchdog.service -u blockuntu-hosts.service',
        'suite.log': 'journalctl --user -b --no-pager -u blockuntu-acceptance-suite',
        'gui.log': 'journalctl --user -b --no-pager -u blockuntu-acceptance-gui',
        'environment.txt': 'cat /etc/os-release; uname -r; id; loginctl list-sessions --no-legend',
        'unit-state.json': 'cat Testing/layer3/unit-state.json',
        'units.txt': 'systemctl show blockuntu.socket blockuntu.service blockuntu-watchdog.service blockuntu-hosts.path -p Id -p ActiveState -p UnitFileState -p NRestarts',
        'guest-results.json': 'cat Testing/layer3/result.json',
        'gui-health.json': 'cat Testing/layer3/gui-health.json',
        'export.toml': 'cat Testing/layer3/export.toml',
        'browser-metadata.json': 'cat Testing/layer3/browser-metadata.json',
        'firefox-failure.json': 'cat Testing/layer3/firefox-failure.json',
        'chrome-failure.json': 'cat Testing/layer3/chrome-failure.json',
        'prerequisites.log': 'cat Testing/layer3/prerequisites.log',
        'accessibility-status.txt': 'cat Testing/layer3/accessibility-status.txt',
        'keyboard-layout.txt': 'cat Testing/layer3/keyboard-layout.txt',
        'process.log': 'cat Testing/layer3/process.log',
        'process-results.json': 'cat Testing/layer3/process-results.json',
        'process-completion.json': 'cat Testing/layer3/process-completion.json',
        'process-journal.log': 'sudo -n journalctl -b --no-pager -u blockuntu-process-acceptance',
    }.items():
        try:
            answer = vm.ssh(record, command, check=False)
            (destination / name).write_text(answer.stdout + answer.stderr)
        except Exception as error:
            (destination / name).write_text(str(error))
    for browser in ('firefox', 'chrome'):
        for cid in ('vm-br-001', 'vm-br-002', 'vm-web-002', 'vm-wal-004'):
            name = f'{browser}-{cid}-failure.json'
            answer = vm.ssh(record, 'cat Testing/layer3/' + name, check=False)
            if answer.returncode == 0:
                (destination / name).write_text(answer.stdout)
    vm.virsh('screenshot', record['uuid'], destination / 'desktop.png', check=False)


def exercise_guest(record, destination, run_id, result):
    upload(record, ROOT / 'fixtures/test-site/server.py', '/home/akhi/Testing/layer3/server.py')
    for source in (ROOT / 'fixtures/layer3').glob('*.py'):
        upload(record, source, '/home/akhi/Testing/layer3/' + source.name)
    for source in (ROOT / 'artifacts/fixtures/bin').glob('blockuntu-test-*'):
        upload(record, source, '/home/akhi/Testing/layer3/' + source.name.replace('blockuntu-test-', 'bk-test-'))
    vm.ssh(record, 'chmod +x Testing/layer3/bk-test-*')
    inhibitor = 'kde-inhibit --power --screenSaver ' if record['template'] == 'cachyos' else ''
    vm.ssh(record, 'systemd-run --user --collect --unit=blockuntu-acceptance-suite '
           '--property=RuntimeMaxSec=1800 ' + inhibitor +
           '/usr/bin/python3 /home/akhi/Testing/layer3/guest.py --run-id '
           + shlex.quote(run_id) + ' --guest ' + shlex.quote(record['template']))
    deadline = time.monotonic() + 1800
    handled = set()
    reported = set()
    while time.monotonic() < deadline:
        request = vm.ssh(record, 'cat Testing/layer3/host-action.json 2>/dev/null', check=False).stdout
        if request:
            action = json.loads(request)
            if action['id'] not in handled:
                if action['action'] == 'keys':
                    keys = action['keys']
                    allowed = [('KEY_LEFTCTRL', 'KEY_L'), ('KEY_ENTER',), ('KEY_ESC',)]
                    if tuple(keys) not in allowed:
                        raise ValueError('Guest requested non-whitelisted key sequence')
                    vm.guarded(record)
                    vm.virsh('send-key', record['uuid'], '--codeset', 'linux', *keys)
                elif action['action'] == 'screenshot':
                    if not re.fullmatch(r'[a-z0-9-]+', action['name']):
                        raise ValueError('Invalid screenshot basename')
                    vm.virsh('screenshot', record['uuid'], destination / (action['name'] + '.png'))
                elif action['action'] == 'type_url':
                    sequences = url_keys(action['url'])
                    vm.guarded(record)
                    # Delete only a selected inline autocomplete suffix (or nothing
                    # at end of input) before committing the exact fixture URL.
                    for keys in [('KEY_LEFTCTRL', 'KEY_L'), *sequences, ('KEY_DELETE',), ('KEY_ENTER',)]:
                        vm.virsh('send-key', record['uuid'], '--codeset', 'linux', '--holdtime', '20', *keys)
                        time.sleep(0.04)
                else:
                    raise ValueError('Unsupported guest host action')
                handled.add(action['id'])
                vm.ssh(record, 'cat > Testing/layer3/host-action-done.json', input=json.dumps({'id': action['id']}))
        answer = vm.ssh(record, 'cat Testing/layer3/result.json 2>/dev/null', check=False)
        if answer.stdout:
            guest = json.loads(answer.stdout)
            for row in guest['cases']:
                key = (row['id'], row.get('browser'))
                if key not in reported:
                    print(f"{row['id']} {row.get('browser', '')}: {row['status']} "
                          f"{row.get('error', row.get('reason', ''))}", flush=True)
                    reported.add(key)
            vm.save(destination / 'result.json', {**result, 'cases': result['cases'] + guest['cases']})
            if guest['status'] != 'running':
                break
        state = vm.ssh(record, 'systemctl --user is-active blockuntu-acceptance-suite', check=False)
        if state.stdout.strip() not in ('active', 'activating'):
            answer = vm.ssh(record, 'cat Testing/layer3/result.json 2>/dev/null', check=False)
            if answer.stdout:
                guest = json.loads(answer.stdout)
                if guest['status'] != 'running':
                    break
            raise RuntimeError('Guest suite exited before publishing a terminal result; see suite journal')
        time.sleep(1)
    else:
        raise TimeoutError('Guest suite exceeded 1800 seconds')
    return guest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--guest', choices=sorted(PLATFORMS), default='ubuntu')
    parser.add_argument('--package-report', type=Path, required=True)
    parser.add_argument('--prepared', action='store_true', help='Use an existing untouched, prepared clone')
    parser.add_argument('--keep', action='store_true', help='Retain stopped clone even on success')
    parser.add_argument('--refresh-cachyos', action='store_true',
                        help='Fully update the CachyOS clone before acceptance; default uses the supplied baseline')
    args = parser.parse_args()
    if args.refresh_cachyos and args.guest != 'cachyos':
        parser.error('--refresh-cachyos requires --guest cachyos')
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,39}', args.run_id):
        parser.error('Run ID requires 1-40 lowercase letters, digits or hyphens')
    package, report = artifact(args.package_report, args.guest)
    platform = PLATFORMS[args.guest]
    vm.RUNTIME.mkdir(parents=True, exist_ok=True)
    with (vm.RUNTIME / 'runner.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        vm.guestfs_setup()
        if args.prepared:
            record = json.loads((vm.RUNTIME / args.run_id / args.guest / 'owner.json').read_text())
            if (record['stage'] != 'prepared' or record['run_id'] != args.run_id or
                    record['template'] != args.guest):
                raise ValueError(f'Expected matching prepared {args.guest} clone')
        else:
            record = vm.prepare(args.guest, args.run_id)
        destination = Path(record['evidence']) / 'acceptance'
        destination.mkdir(exist_ok=False)
        destination.chmod(0o700)
        vm.save(destination / 'package-report.json', report)
        vm.save(destination / 'harness-files.json', {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in [Path(__file__), ROOT / 'scripts/phase0-vm.py', ROOT / 'fixtures/test-processes/test_process.c',
                          ROOT / 'fixtures/test-site/server.py', *sorted((ROOT / 'fixtures/layer3').glob('*.py'))]})
        result = {'schema_version': 1, 'layer': 'vm-acceptance', 'suite': f'{args.guest}-smoke',
                  'guest': args.guest, 'refresh_cachyos': args.refresh_cachyos,
                  'run_id': args.run_id, 'package_commit': report['commit'],
                  'package': report['artifacts'], 'harness_commit': vm.run(['git', 'rev-parse', 'HEAD']).stdout.strip(),
                  'harness_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  'started_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'status': 'running', 'cases': []}
        try:
            # Phase 0 verifies pristine baseline, identities, desktop and reboot.
            vm.run(['bash', ROOT / 'scripts/build-test-processes.sh'])
            vm.verify(record)
            boot(record)
            if args.guest == 'cachyos':
                print('cachyos: checking supplied baseline' + (' and refreshing the rolling guest' if args.refresh_cachyos else ''), flush=True)
                prepare_cachyos(record, destination, refresh=args.refresh_cachyos)
            print(f'{args.guest}: infrastructure passed; installing exact CI artifact', flush=True)
            vm.ssh(record, 'mkdir -p Testing/layer3; chmod 700 Testing/layer3')
            vm.ssh(record, 'sudo -n tee -a /etc/hosts >/dev/null',
                   input='\n# BlocKuntu acceptance fixtures (outside managed block)\n127.0.0.1 web.blockuntu.test allowed.blockuntu.test outside.blockuntu.test\n')
            upload(record, package, platform['remote'])
            actual = vm.ssh(record, 'sha256sum ' + shlex.quote(platform['remote'])).stdout.split()[0]
            if actual != next(a['sha256'] for a in report['artifacts'] if a['filename'] == package.name):
                raise ValueError('Guest package checksum mismatch')
            installation = vm.ssh(
                record,
                platform['install'].format(package=shlex.quote(platform['remote'])),
                timeout=900,
                check=False,
            )
            (destination / 'install.log').write_text(installation.stdout + installation.stderr)
            if installation.returncode:
                raise RuntimeError('Package installation failed; see install.log')
            version = vm.ssh(record, platform['version']).stdout.strip()
            if version != report['package_metadata']['version']:
                raise ValueError('Installed version differs from artifact metadata')
            result['cases'].append({'id': 'VM-INSTALL-001', 'status': 'pass', 'version': version})
            vm.ssh(record, 'sudo -n usermod -aG blockuntu akhi')
            if args.guest == 'cachyos':
                # KWin reads kxkbrc when the graphical session starts. Write
                # it before the required post-install reboot, not in guest.py.
                vm.ssh(record, 'kwriteconfig6 --file kxkbrc --group Layout --key Use true')
                vm.ssh(record, 'kwriteconfig6 --file kxkbrc --group Layout --key LayoutList us')
            boot_id = vm.ssh(record, 'cat /proc/sys/kernel/random/boot_id').stdout.strip()
            vm.ssh(record, 'sudo -n systemctl reboot', check=False)
            time.sleep(5)
            def rebooted():
                record['ip'] = vm.address(record) or record['ip']
                answer = vm.ssh(record, 'cat /proc/sys/kernel/random/boot_id', check=False)
                return answer.returncode == 0 and answer.stdout.strip() != boot_id
            vm.wait_until(rebooted, 'post-install reboot')
            vm.graphical_check(record, '-installed')
            vm.ssh(record, 'id -nG | tr " " "\\n" | grep -qx blockuntu')
            guest = exercise_guest(record, destination, args.run_id, result)
            result['cases'].extend(guest['cases'])
            specified = set(re.findall(r'^\| (VM-[A-Z]+-\d+) \|', (ROOT / 'layers/03-vm-acceptance.md').read_text(), re.M))
            observed = {row['id'] for row in result['cases']}
            result['cases'].extend({'id': cid, 'status': 'planned',
                                    'reason': f'Outside initial {args.guest} smoke subset'}
                                   for cid in sorted(specified - observed))
            result['status'] = ('fail' if any(c['status'] == 'fail' for c in guest['cases'])
                                else 'pass' if guest['status'] == 'pass' else 'incomplete')
        except Exception as error:
            result.update(status='fail', error=str(error))
            print(str(error), file=sys.stderr, flush=True)
        finally:
            try:
                if vm.virsh('domstate', record['uuid']).stdout.strip() != 'shut off':
                    collect(record, destination)
            finally:
                try:
                    stop(record)
                    result['base_integrity_verified'] = True
                except Exception as error:
                    result.update(status='fail', cleanup_error=str(error))
                result['finished_at'] = dt.datetime.now(dt.timezone.utc).isoformat()
                vm.save(destination / 'result.json', result)
        if result['status'] == 'pass' and not args.keep:
            vm.cleanup(record)
        print(f"Acceptance {result['status']}: {destination / 'result.json'}", flush=True)
        return 0 if result['status'] == 'pass' else 1


if __name__ == '__main__':
    sys.exit(main())
