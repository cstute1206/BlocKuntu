#!/usr/bin/env python3
"""Run installed-package acceptance on an owned disposable libvirt clone."""
from __future__ import annotations

import argparse
import base64
import datetime as dt
import fcntl
import hashlib
import importlib.util
import inspect
import json
from pathlib import Path
import re
import shlex
import sys
import time

from layer3_contract import SUITES, assess_observations, load_contract
from layer3_runtime import (evaluate_smoke, execution_plans, smoke_budget_seconds,
                            process_budget_seconds, group_budget_seconds, isolated_case,
                            P32_DOMAIN_CASES, p32_website_budget_seconds,
                            p32_website_plan)

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
        url in ('about:policies', 'chrome://policy',
                'https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc') or
        re.fullmatch(r'chrome://extensions/\?id=[a-p]{32}', url) or
        re.fullmatch(
            r'http://(?:(?:web|allowed|outside)\.blockuntu\.test|'
            r'(?:(?:www\.)?(?:firefox|chrome)|near-(?:firefox|chrome))\.web\.blockuntu\.test)'
            r':18080/[a-z/]+(?:\?marker=[a-z-]+)?', url)
    ):
        raise ValueError('Guest requested a URL outside the acceptance fixture')
    punctuation = {'.': ('KEY_DOT',), '/': ('KEY_SLASH',),
                   ':': ('KEY_LEFTSHIFT', 'KEY_SEMICOLON'), '-': ('KEY_MINUS',),
                   '?': ('KEY_LEFTSHIFT', 'KEY_SLASH'), '=': ('KEY_EQUAL',)}
    return [punctuation[c] if c in punctuation else ('KEY_' + c.upper(),) for c in url]


def validate_host_action(request):
    """Accept only the documented fixture UI actions and their exact fields."""
    if (not isinstance(request, dict) or not isinstance(request.get('id'), str) or
            not re.fullmatch(r'[0-9a-f]{32}', request['id'])):
        raise ValueError('Invalid guest host-action ID')
    action = request.get('action')
    fields = {'keys': {'id', 'action', 'keys'},
              'screenshot': {'id', 'action', 'name'},
              'type_url': {'id', 'action', 'url'},
              'type_form': {'id', 'action', 'value'}}
    if not isinstance(action, str) or action not in fields or set(request) != fields[action]:
        raise ValueError('Unsupported or malformed guest host action')
    if action == 'keys':
        allowed = {('KEY_LEFTCTRL', 'KEY_L'), ('KEY_LEFTCTRL', 'KEY_1'), ('KEY_LEFTCTRL', 'KEY_2'),
                   ('KEY_LEFTCTRL', 'KEY_LEFTSHIFT', 'KEY_P'),
                   ('KEY_LEFTCTRL', 'KEY_LEFTSHIFT', 'KEY_N'),
                   ('KEY_LEFTALT', 'KEY_F4'),
                   ('KEY_LEFTALT', 'KEY_TAB'),
                   ('KEY_HOME',), ('KEY_DOWN',), ('KEY_TAB',), ('KEY_SPACE',),
                   ('KEY_ENTER',), ('KEY_ESC',)}
        keys = request['keys']
        if not isinstance(keys, list) or any(not isinstance(key, str) for key in keys) or tuple(keys) not in allowed:
            raise ValueError('Guest requested non-whitelisted key sequence')
    elif action == 'screenshot':
        name = request['name']
        if not isinstance(name, str) or len(name) > 80 or not re.fullmatch(r'[a-z0-9-]+', name):
            raise ValueError('Invalid screenshot basename')
    elif action == 'type_form':
        form_keys(request['value'])
    else:
        url_keys(request['url'])
    return request


def form_keys(value):
    """Encode only the names/matchers authored by the installed-GUI fixture."""
    if isinstance(value,str) and value.startswith('http://'):
        return url_keys(value)
    if not isinstance(value,str) or not re.fullmatch(
        r'Acceptance GUI (?:website|application) (?:blocklist|allowlist)|'
        r'/home/akhi/Testing/layer3/bk-test-app-[ab]|bk-test-app-[ab]|bk-test-block',value):
        raise ValueError('Guest requested form input outside the acceptance fixture')
    punctuation={' ':('KEY_SPACE',),'/':('KEY_SLASH',),'-':('KEY_MINUS',)}
    return [punctuation[c] if c in punctuation else
            ('KEY_LEFTSHIFT','KEY_'+c.upper()) if c.isupper() else
            ('KEY_'+c.upper(),) for c in value]


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


def clean_pacman_staging(cache, lock):
    """Remove inactive pacman download staging directories from a cloned cache."""
    import os
    from pathlib import Path
    import re
    import shutil
    import subprocess
    cache, lock = Path(cache), Path(lock)
    if lock.exists() or subprocess.run(['pgrep', '-x', 'pacman'],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
        raise ValueError('Pacman is active; refusing cache staging cleanup')
    if (cache.is_symlink() or cache.resolve() != cache or not cache.is_dir() or
            cache.stat().st_uid != os.geteuid()):
        raise ValueError('Unexpected pacman cache path')
    if not shutil.rmtree.avoids_symlink_attacks:
        raise ValueError('Safe staging-directory removal is unavailable')
    selected = sorted(p for p in cache.iterdir()
                      if re.fullmatch(r'download-[A-Za-z0-9]{6}', p.name))
    # Validate every target before deleting any, including symlink targets.
    for path in selected:
        if path.is_symlink() or not path.is_dir():
            raise ValueError('Unexpected pacman staging entry: ' + path.name)
    for path in selected:
        shutil.rmtree(path)
    return {'removed': [p.name for p in selected], 'cache': str(cache)}


def prepare_cachyos_keyboard(record, destination):
    """KWin loads the cloned user's fixture layout at the next graphical login."""
    if record['template'] != 'cachyos':
        raise ValueError('CachyOS keyboard preparation requires a CachyOS clone')
    rows=[]
    for command in ('kwriteconfig6 --file kxkbrc --group Layout --key Use true',
                    'kwriteconfig6 --file kxkbrc --group Layout --key LayoutList us'):
        vm.guarded(record)
        answer=vm.ssh(record, command)
        rows.append({'command':command,'returncode':answer.returncode,
                     'stdout':answer.stdout,'stderr':answer.stderr})
    (destination/'cachyos-keyboard-preparation.json').write_text(json.dumps(
        {'run_id':record['run_id'],'clone_uuid':record['uuid'],'commands':rows},indent=2)+'\n')


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
        vm.guarded(record)
        code = (inspect.getsource(clean_pacman_staging) +
                "\nimport json\nprint(json.dumps(clean_pacman_staging("
                "'/var/cache/pacman/pkg', '/var/lib/pacman/db.lck')))\n")
        staging = vm.ssh(record, 'sudo -n /usr/bin/python3 -c ' + shlex.quote(code),
                         timeout=120, check=False)
        (destination / f'pacman-staging-{stage}.json').write_text(staging.stdout)
        if staging.returncode:
            (destination / f'pacman-staging-{stage}.error.log').write_text(staging.stderr)
            raise RuntimeError('Inactive pacman staging cleanup failed')
        json.loads(staging.stdout)
        vm.guarded(record)
        cleaned = vm.ssh(record, "printf 'y\\ny\\n' | sudo -n env LC_ALL=C pacman -Scc",
                         timeout=120, check=False)
        (destination / name).write_text(cleaned.stdout + cleaned.stderr)
        if cleaned.returncode:
            raise RuntimeError(f'CachyOS package cache cleanup failed; see {name}')
    clean_cache('before-update')
    vm.guarded(record)
    updated = vm.ssh(record, 'sudo -n pacman -Syyu --noconfirm', timeout=1800, check=False)
    (destination / 'os-update.log').write_text(updated.stdout + updated.stderr)
    if updated.returncode:
        raise RuntimeError('CachyOS system update failed; see os-update.log')
    clean_cache('after-update')
    boot_id = vm.ssh(record, 'cat /proc/sys/kernel/random/boot_id').stdout.strip()
    vm.guarded(record)
    vm.ssh(record, 'sudo -n systemctl reboot', check=False)
    time.sleep(5)
    def rebooted():
        record['ip'] = vm.address(record) or record['ip']
        answer = vm.ssh(record, 'cat /proc/sys/kernel/random/boot_id', check=False)
        return answer.returncode == 0 and answer.stdout.strip() != boot_id
    vm.wait_until(rebooted, 'post-update CachyOS reboot')
    vm.graphical_check(record, '-updated')


def collect(record, destination, plan, extra_plan=None):
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
        'chromium-failure.json': 'cat Testing/layer3/chromium-failure.json',
        'prerequisites.log': 'cat Testing/layer3/prerequisites.log',
        'accessibility-status.txt': 'cat Testing/layer3/accessibility-status.txt',
        'keyboard-layout.txt': 'cat Testing/layer3/keyboard-layout.txt',
        'domain-dns.json': 'cat Testing/layer3/domain-dns.json',
        'domain-dns-journal.log': 'sudo -n journalctl -b --no-pager -u blockuntu-p32-dns.service',
        'application-lifecycle.json': 'cat Testing/layer3/application-lifecycle.json',
        'application-lifecycle-journal.log': 'sudo -n journalctl -b --no-pager -u blockuntu-application-lifecycle',
        'helper-result.json': 'cat Testing/layer3/helper-result.json',
        'helper-journal.log': 'sudo -n journalctl -b --no-pager -u blockuntu-helper-acceptance',
        'gui-edits-failure.json': 'cat Testing/layer3/gui-edits-failure.json',
        'gui-session-inventory.json': 'cat Testing/layer3/gui-session-inventory.json',
        'p32-result.json': 'cat Testing/layer3/p32-result.json',
        'private-failure.json': 'cat Testing/layer3/private-failure.json',
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
    failures = {f"{row['browser_package'].split('-')[0]}-{row['id'].lower()}-failure.json"
                for selected in (plan, extra_plan) if selected is not None
                for row in selected['observations'] if row['browser_package']}
    for name in sorted(failures):
        answer = vm.ssh(record, 'cat Testing/layer3/' + name, check=False)
        if answer.returncode == 0:
            (destination / name).write_text(answer.stdout)
    vm.virsh('screenshot', record['uuid'], destination / 'desktop.png', check=False)


def exercise_guest(record, destination, run_id, result, plan, extra_plan=None):
    budget = (p32_website_budget_seconds(plan, extra_plan) if extra_plan is not None
              else smoke_budget_seconds(plan))
    upload(record, destination / 'smoke-plan.json', '/home/akhi/Testing/layer3/smoke-plan.json')
    if extra_plan is not None:
        vm.save(destination / 'p32-plan.json', extra_plan)
        upload(record, destination / 'p32-plan.json', '/home/akhi/Testing/layer3/p32-plan.json')
    for module in ('layer3_contract.py', 'layer3_runtime.py'):
        upload(record, ROOT / 'scripts' / module, '/home/akhi/Testing/layer3/' + module)
    upload(record, ROOT / 'fixtures/test-site/server.py', '/home/akhi/Testing/layer3/server.py')
    for source in (ROOT / 'fixtures/layer3').glob('*.py'):
        upload(record, source, '/home/akhi/Testing/layer3/' + source.name)
    for source in (ROOT / 'artifacts/fixtures/bin').glob('blockuntu-test-*'):
        upload(record, source, '/home/akhi/Testing/layer3/' + source.name.replace('blockuntu-test-', 'bk-test-'))
    vm.ssh(record, 'chmod +x Testing/layer3/bk-test-*')
    inhibitor = 'kde-inhibit --power --screenSaver ' if record['template'] == 'cachyos' else ''
    vm.ssh(record, 'systemd-run --user --collect --unit=blockuntu-acceptance-suite '
           '--property=RuntimeMaxSec=' + str(budget) + ' ' + inhibitor +
           '/usr/bin/python3 /home/akhi/Testing/layer3/guest.py --run-id '
           + shlex.quote(run_id) + ' --guest ' + shlex.quote(record['template']))
    deadline = time.monotonic() + budget
    handled = {}
    reported = set()
    while time.monotonic() < deadline:
        request = vm.ssh(record, 'cat Testing/layer3/host-action.json 2>/dev/null', check=False).stdout
        if request:
            action = validate_host_action(json.loads(request))
            if action['id'] in handled and action != handled[action['id']]:
                raise ValueError('Guest reused a host-action ID with different content')
            if action['id'] not in handled:
                if action['action'] == 'keys':
                    keys = action['keys']
                    vm.guarded(record)
                    vm.virsh('send-key', record['uuid'], '--codeset', 'linux', *keys)
                elif action['action'] == 'screenshot':
                    vm.virsh('screenshot', record['uuid'], destination / (action['name'] + '.png'))
                elif action['action'] in ('type_url','type_form'):
                    form=action['action']=='type_form'
                    sequences = form_keys(action['value']) if form else url_keys(action['url'])
                    vm.guarded(record)
                    # Delete only a selected inline autocomplete suffix (or nothing
                    # at end of input) before committing the exact fixture URL.
                    keys_to_send=([('KEY_LEFTCTRL','KEY_A'),*sequences,('KEY_TAB',)] if form else
                                  [('KEY_LEFTCTRL','KEY_L'),*sequences,('KEY_DELETE',),('KEY_ENTER',)])
                    for keys in keys_to_send:
                        vm.virsh('send-key', record['uuid'], '--codeset', 'linux', '--holdtime', '20', *keys)
                        time.sleep(0.04)
                else:
                    raise ValueError('Unsupported guest host action')
                handled[action['id']] = action
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
        raise TimeoutError(f'Guest suite exceeded its planned {budget} seconds')
    return guest


def exercise_scenario(record, destination, run_id, plan):
    """Execute one bounded, independently checked guest-only scenario."""
    row = plan['observations'][0]
    if len(plan['observations']) != 1 or not isolated_case(row, plan):
        raise ValueError('No isolated scenario executor for this selection')
    for name in ('layer3_contract.py', 'layer3_runtime.py'):
        upload(record, ROOT / 'scripts' / name, '/home/akhi/Testing/layer3/' + name)
    for name in ('guest.py', 'scenarios.py', 'lifecycle.py', 'application_lifecycle.py','session_inventory.py'):
        upload(record, ROOT / 'fixtures/layer3' / name, '/home/akhi/Testing/layer3/' + name)
    if row['id'] == 'VM-AAL-010':
        for source in (ROOT / 'artifacts/fixtures/bin').glob('blockuntu-test-*'):
            upload(record, source, '/home/akhi/Testing/layer3/' + source.name.replace('blockuntu-test-', 'bk-test-'))
        vm.ssh(record, 'chmod +x Testing/layer3/bk-test-*')
    command = ('cd Testing/layer3 && /usr/bin/python3 scenarios.py --run-id ' +
               shlex.quote(run_id) + ' --scenario ' + shlex.quote(row['id']))
    answer = vm.ssh(record, command, check=False, timeout=row['budget_seconds'])
    (destination / 'scenario.log').write_text(answer.stdout + answer.stderr)
    remote = vm.ssh(record, 'cat Testing/layer3/scenario-result.json', check=False)
    if remote.returncode:
        raise RuntimeError('Scenario did not publish a result; see scenario.log')
    (destination / 'scenario-result.json').write_text(remote.stdout)
    result = json.loads(remote.stdout)
    if result.get('id') != row['id'] or result.get('status') not in ('pass', 'fail'):
        raise ValueError('Scenario published an invalid result')
    if answer.returncode != int(result['status'] != 'pass'):
        raise ValueError('Scenario exit code differs from its published result')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--guest', choices=sorted(PLATFORMS), default='ubuntu')
    parser.add_argument('--package-report', type=Path, required=True)
    parser.add_argument('--suite', choices=SUITES, default='smoke',
                        help='Required Layer 3 coverage; unimplemented selections fail before VM creation')
    parser.add_argument('--case', action='append', dest='case_ids', metavar='CASE-ID',
                        help='Select an implemented isolated case or project a complete smoke execution')
    parser.add_argument('--browser', action='append', choices=('firefox-native', 'chrome-native'),
                        help='Select one browser for a fresh-clone unlock journey')
    parser.add_argument('--scenario', action='append', help='Representative scenarios for explicit selected cases')
    parser.add_argument('--prepared', action='store_true', help='Use an existing untouched, prepared clone')
    parser.add_argument('--keep', action='store_true', help='Retain stopped clone even on success')
    parser.add_argument('--refresh-cachyos', action='store_true',
                        help='Fully update the CachyOS clone before acceptance; default uses the supplied baseline')
    args = parser.parse_args()
    if args.refresh_cachyos and args.guest != 'cachyos':
        parser.error('--refresh-cachyos requires --guest cachyos')
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]{0,39}', args.run_id):
        parser.error('Run ID requires 1-40 lowercase letters, digits or hyphens')
    try:
        registry, matrix = load_contract()
        smoke_plan, selected_plan = execution_plans(registry, matrix, guest=args.guest,
            suite=args.suite, case_ids=args.case_ids, browsers=args.browser, scenarios=args.scenario)
    except ValueError as error:
        parser.error(str(error))
    package, report = artifact(args.package_report, args.guest)
    platform = PLATFORMS[args.guest]
    isolated = (args.case_ids is not None and len(args.case_ids) == 1 and
                len(selected_plan['observations']) == 1 and
                isolated_case(selected_plan['observations'][0], selected_plan))
    p32 = p32_website_plan(selected_plan)
    domain_group = p32 and {row['id'] for row in selected_plan['observations']} == P32_DOMAIN_CASES
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
        vm.save(destination / 'smoke-plan.json', smoke_plan)
        vm.save(destination / 'selected-plan.json', selected_plan)
        vm.save(destination / 'harness-files.json', {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in [Path(__file__), ROOT / 'scripts/phase0-vm.py', ROOT / 'fixtures/test-processes/test_process.c',
                          ROOT / 'fixtures/test-site/server.py', ROOT / 'scripts/layer3_contract.py',
                          ROOT / 'scripts/layer3_runtime.py', *sorted((ROOT / 'fixtures/layer3').glob('*.py'))]})
        result = {'schema_version': 1, 'layer': 'vm-acceptance',
                  'suite': f'{args.guest}-{args.suite}' + ('-selected' if args.case_ids else ''),
                  'coverage_scope': selected_plan['scope'], 'selected_case_ids': args.case_ids,
                  'guest': args.guest, 'refresh_cachyos': args.refresh_cachyos,
                  'run_id': args.run_id, 'package_commit': report['commit'],
                  'package': report['artifacts'], 'harness_commit': vm.run(['git', 'rev-parse', 'HEAD']).stdout.strip(),
                  'harness_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  'started_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'status': 'running', 'cases': []}
        result['budgets_seconds'] = ({'scenario_group': group_budget_seconds(
            selected_plan, [selected_plan['observations'][0]['observation_id']])} if isolated else
            {'smoke_group': smoke_budget_seconds(smoke_plan),
             'process_supervisor': process_budget_seconds(smoke_plan),
             **({'website_group': p32_website_budget_seconds(smoke_plan, selected_plan)}
                if p32 else {})})
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
            fixture_hosts = ('allowed.blockuntu.test outside.blockuntu.test' if domain_group else
                             'web.blockuntu.test allowed.blockuntu.test outside.blockuntu.test')
            vm.ssh(record, 'sudo -n tee -a /etc/hosts >/dev/null',
                   input='\n# BlocKuntu acceptance fixtures (outside managed block)\n127.0.0.1 ' + fixture_hosts + '\n')
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
                prepare_cachyos_keyboard(record, destination)
            if args.guest == 'ubuntu' and not isolated:
                for name in ('layer3_contract.py', 'layer3_runtime.py'):
                    upload(record, ROOT / 'scripts' / name, '/home/akhi/Testing/layer3/' + name)
                for name in ('guest.py', 'reboot_policy.py'):
                    upload(record, ROOT / 'fixtures/layer3' / name, '/home/akhi/Testing/layer3/' + name)
                seeded = vm.ssh(record, 'sudo -n -u akhi /usr/bin/python3 '
                    '/home/akhi/Testing/layer3/reboot_policy.py --run-id ' + shlex.quote(args.run_id))
                (destination / 'reboot-policy.json').write_text(seeded.stdout)
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
            if isolated:
                scenario = exercise_scenario(record, destination, args.run_id, selected_plan)
                result['cases'].append(scenario)
                required = selected_plan['observations'][0]
                result['observations'] = [{**scenario,
                    'observation_id': required['observation_id'], 'guest': required['guest'],
                    'browser_package': required['browser_package'], 'scenario': required['scenario']}]
                result['selected_coverage'] = assess_observations(selected_plan, result['observations'])
                result['status'] = result['selected_coverage']['observation_status']
            else:
                guest = exercise_guest(record, destination, args.run_id, result, smoke_plan,
                                       selected_plan if p32 else None)
                result['cases'].extend(guest['cases'])
                smoke_observations, result['smoke_coverage'] = evaluate_smoke(smoke_plan, result['cases'])
                if p32:
                    remote = vm.ssh(record, 'cat Testing/layer3/p32-result.json', check=False)
                    if remote.returncode:
                        raise RuntimeError('Website group did not publish a checkpoint')
                    (destination / 'p32-result.json').write_text(remote.stdout)
                    result['p32_cases'] = json.loads(remote.stdout)
                    expected = {(row['id'], row['browser_package'].removesuffix('-native')
                                 if row['browser_package'] else None, row['scenario']): row
                                for row in selected_plan['observations']}
                    result['observations'] = []
                    for index, row in enumerate(result['p32_cases']):
                        match = expected.get((row.get('id'), row.get('browser'), row.get('scenario', 'baseline')))
                        if match:
                            result['observations'].append({**row,
                                'observation_id': match['observation_id'], 'guest': match['guest'],
                                'browser_package': match['browser_package'], 'scenario': match['scenario']})
                        else:
                            result['observations'].append({**row,
                                'observation_id': f'unexpected:{index}', 'guest': args.guest,
                                'browser_package': None, 'scenario': None})
                    result['smoke_observations'] = smoke_observations
                    result['selected_coverage'] = assess_observations(selected_plan, result['observations'])
                else:
                    result['observations'] = smoke_observations
                    selected_cases = [row for row in result['cases'] if row.get('id') in
                                      {entry['id'] for entry in selected_plan['observations']} or
                                      row.get('id', '').startswith('SETUP-')]
                    _, result['selected_coverage'] = evaluate_smoke(selected_plan, selected_cases)
            specified = set(re.findall(r'^\| (VM-[A-Z]+-\d+) \|', (ROOT / 'layers/03-vm-acceptance.md').read_text(), re.M))
            observed = {row['id'] for row in result['cases']}
            if p32:
                observed.update(row['id'] for row in result['p32_cases'])
            result['cases'].extend({'id': cid, 'status': 'planned',
                                    'reason': f'Outside initial {args.guest} smoke subset'}
                                   for cid in sorted(specified - observed))
            if not isolated:
                result['status'] = result['smoke_coverage']['observation_status']
                if guest['status'] != 'pass' or result['selected_coverage']['observation_status'] != 'pass':
                    result['status'] = 'fail' if 'fail' in (guest['status'],
                        result['selected_coverage']['observation_status']) else 'incomplete'
            result['acceptance_status'] = 'not_evaluated'
        except Exception as error:
            result.update(status='fail', error=str(error))
            print(str(error), file=sys.stderr, flush=True)
        finally:
            try:
                if vm.virsh('domstate', record['uuid']).stdout.strip() != 'shut off':
                    collect(record, destination, smoke_plan, selected_plan if p32 else None)
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
