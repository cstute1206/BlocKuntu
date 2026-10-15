#!/usr/bin/env python3
"""Guest-side checks. Invoked only by the clone-guarded host runner."""
import argparse
import ipaddress
import json
import os
import re
from pathlib import Path
import socket
import subprocess
import time
import traceback

from layer3_contract import build_plan, load_contract
from layer3_runtime import (P32_DOMAIN_CASES, GUI_EDIT_CASES, HELPER_CASES, evaluate_smoke, p32_website_plan,
                            process_budget_seconds, require_stage_passed)

WORK = Path('/home/akhi/Testing/layer3')


def run(*args, check=True, timeout=60):
    return subprocess.run(args, text=True, capture_output=True, check=check, timeout=timeout)


def rpc(method, params=None):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(5)
        client.connect('/run/blockuntu/blockuntud.sock')
        client.sendall(json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': params or {}}).encode())
        client.shutdown(socket.SHUT_WR)
        chunks = []
        while chunk := client.recv(65536):
            chunks.append(chunk)
    reply = json.loads(b''.join(chunks))
    if 'error' in reply:
        raise RuntimeError(json.dumps(reply['error']))
    return reply['result']


def wait(predicate, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(1)
    raise TimeoutError(f'Condition not reached within {seconds}s')


def save_result(result):
    temporary = WORK / 'result.tmp'
    temporary.write_text(json.dumps(result, indent=2))
    temporary.replace(WORK / 'result.json')


PROCESS_IDS = ('VM-APP-001', 'VM-APP-002', 'VM-APP-003', 'VM-AAL-004')


def process_rows(path):
    """Read a supervisor checkpoint without turning partial work into a pass."""
    rows = json.loads(Path(path).read_text())
    if (not isinstance(rows, list) or len(rows) > len(PROCESS_IDS) or
            any(not isinstance(row, dict) or row.get('id') != PROCESS_IDS[index] or
                row.get('status') not in ('pass', 'fail') or
                not isinstance(row.get('seconds'), (int, float)) or row['seconds'] < 0
                for index, row in enumerate(rows))):
        raise ValueError('Invalid process supervisor checkpoint')
    return rows


def host_action(action, **kwargs):
    import uuid
    request = {'id': uuid.uuid4().hex, 'action': action, **kwargs}
    temporary = WORK / 'host-action.tmp'
    temporary.write_text(json.dumps(request))
    temporary.replace(WORK / 'host-action.json')
    def complete():
        try:
            return json.loads((WORK / 'host-action-done.json').read_text())['id'] == request['id']
        except (FileNotFoundError, ValueError):
            return False
    wait(complete, 30)


def setup_domain_dns():
    """Route fixture names through local DNS while forwarding public queries."""
    route = run('ip', '-o', '-4', 'route', 'show', 'default').stdout.splitlines()
    if len(route) != 1 or 'dev' not in route[0].split():
        raise RuntimeError('Domain fixture requires one default network link')
    tokens = route[0].split()
    link = tokens[tokens.index('dev') + 1]
    current = run('resolvectl', 'dns', link).stdout.strip().split(':', 1)
    addresses = current[1].split() if len(current) == 2 else []
    upstream = next((address for address in addresses
                     if ipaddress.ip_address(address).version == 4 and
                     not ipaddress.ip_address(address).is_loopback), None)
    if upstream is None:
        raise RuntimeError('Domain fixture has no existing IPv4 DNS upstream')
    command = ['sudo', '-n', 'systemd-run', '--collect', '--unit=blockuntu-p32-dns',
               '/usr/sbin/dnsmasq', '--no-daemon', '--no-resolv',
               '--server=' + upstream, '--listen-address=127.0.0.1',
               '--bind-interfaces', '--port=53',
               '--address=/web.blockuntu.test/127.0.0.1']
    run(*command)
    wait(lambda: run('systemctl', 'is-active', 'blockuntu-p32-dns.service',
                     check=False).stdout.strip() == 'active', 15)
    run('sudo', '-n', 'resolvectl', 'dns', link, '127.0.0.1')
    run('sudo', '-n', 'resolvectl', 'flush-caches')
    names = ('web.blockuntu.test', 'firefox.web.blockuntu.test',
             'www.firefox.web.blockuntu.test', 'near-firefox.web.blockuntu.test',
             'chrome.web.blockuntu.test', 'www.chrome.web.blockuntu.test',
             'near-chrome.web.blockuntu.test')
    def address(name):
        answer = run('getent', 'ahostsv4', name, check=False)
        return answer.stdout.split()[0] if answer.returncode == 0 else None
    wait(lambda: all(address(name) == '127.0.0.1' for name in names), 30)
    public = address('github.com')
    if public is None:
        raise RuntimeError('Fixture DNS did not forward public queries')
    (WORK / 'domain-dns.json').write_text(json.dumps({
        'link': link, 'upstream': upstream, 'fixture_addresses': {name: address(name) for name in names},
        'public_query_resolved': True,
    }, indent=2) + '\n')



def finalize_result(result, guest='ubuntu'):
    plan_path = WORK / 'smoke-plan.json'
    if plan_path.exists():
        plan = json.loads(plan_path.read_text())
    else:
        # Local unit tests run without a VM-uploaded plan.
        registry, matrix = load_contract()
        plan = build_plan(registry, matrix, suite='smoke', guests=[guest])
    if plan['suite'] != 'smoke' or plan['guests'] != [guest]:
        raise ValueError('Guest received a plan for another suite or guest')
    plan = {**plan, 'observations': [row for row in plan['observations']
                                   if row['id'] != 'VM-INSTALL-001']}
    required = {(row['id'], row['browser_package'].split('-')[0]
                 if row['browser_package'] else None) for row in plan['observations']}
    observed = {(row['id'], row.get('browser')) for row in result['cases']}
    for cid, browser in sorted(required - observed, key=str):
        row = {'id': cid, 'status': 'blocked', 'reason': 'Prerequisite or earlier suite failure prevented execution'}
        if browser:
            row['browser'] = browser
        result['cases'].append(row)
    result['observations'], result['coverage'] = evaluate_smoke(plan, result['cases'])
    result['missing_required_cases'] = [row['id'] + (':' + row['browser_package'].split('-')[0]
                                        if row['browser_package'] else '')
                                        for row in plan['observations']
                                        if row['observation_id'] in result['coverage']['missing_required_observations']]
    result['status'] = result['coverage']['observation_status']

def configure_desktop(guest):
    """Enable native accessibility and verify the fixture keyboard layout."""
    if guest not in ('ubuntu', 'fedora', 'cachyos'):
        raise ValueError('Unsupported desktop guest')
    run('systemctl', '--user', 'set-environment', 'GTK_MODULES=gail:atk-bridge')
    if guest in ('ubuntu', 'fedora'):
        run('gsettings', 'set', 'org.gnome.desktop.interface', 'toolkit-accessibility', 'true')
        run('gsettings', 'set', 'org.gnome.desktop.session', 'idle-delay', '0')
        run('gsettings', 'set', 'org.gnome.desktop.screensaver', 'lock-enabled', 'false')
        run('gsettings', 'set', 'org.gnome.desktop.input-sources', 'sources', "[('xkb', 'us')]")
    else:
        address = ['--session', '--dest', 'org.a11y.Bus', '--object-path', '/org/a11y/bus']
        run('gdbus', 'call', *address, '--method', 'org.freedesktop.DBus.Properties.Set',
            'org.a11y.Status', 'IsEnabled', '<true>')
        enabled = run('gdbus', 'call', *address, '--method',
                      'org.freedesktop.DBus.Properties.Get', 'org.a11y.Status', 'IsEnabled').stdout.strip()
        assert enabled == '(<true>,)', f'KDE accessibility bus not enabled: {enabled}'
        (WORK / 'accessibility-status.txt').write_text(enabled + '\n')
        layouts = run('busctl', '--user', 'call', 'org.kde.keyboard', '/Layouts',
                      'org.kde.KeyboardLayouts', 'getLayoutsList').stdout.strip()
        selected = run('busctl', '--user', 'call', 'org.kde.keyboard', '/Layouts',
                       'org.kde.KeyboardLayouts', 'getLayout').stdout.strip()
        assert '"us"' in layouts and selected == 'u 0', \
            f'KDE session did not activate the US keyboard layout: {layouts}; {selected}'
        (WORK / 'keyboard-layout.txt').write_text(layouts + '\n' + selected + '\n')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--run-id', required=True)
    parser.add_argument('--guest', choices=('ubuntu', 'fedora', 'cachyos'), default='ubuntu')
    args = parser.parse_args()
    result = {'status': 'running', 'cases': []}
    save_result(result)
    p32_path = WORK / 'p32-plan.json'
    p32_plan = json.loads(p32_path.read_text()) if p32_path.exists() else None
    if p32_plan is not None and not p32_website_plan(p32_plan):
        raise ValueError('Guest received an unsupported website group plan')
    p32_rows = []
    def save_p32():
        temporary = WORK / 'p32-result.tmp'
        temporary.write_text(json.dumps(p32_rows, indent=2) + '\n')
        temporary.replace(WORK / 'p32-result.json')
    if p32_plan is not None:
        save_p32()

    def p32_case(case_id, action, browser=None, scenario='baseline'):
        key = (case_id, browser + '-native' if browser else None, scenario)
        required = {(row['id'], row['browser_package'], row['scenario']) for row in p32_plan['observations']}
        if key not in required or any((row['id'], row.get('browser') + '-native' if row.get('browser') else None,
                                      row.get('scenario', 'baseline')) == key for row in p32_rows):
            raise ValueError(f'Unexpected or duplicate website observation: {key}')
        start = time.monotonic()
        row = {'id': case_id, 'browser': browser, 'scenario': scenario}
        try:
            row.update(status='pass', evidence=action())
        except Exception as error:
            row.update(status='fail', error=f'{type(error).__name__}: {error}')
        row['seconds'] = round(time.monotonic() - start, 3)
        p32_rows.append(row)
        save_p32()
        print(case_id, browser, row['status'], row.get('error', ''), flush=True)
        return row['status'] == 'pass'
    def case(case_id, action, browser=None):
        start = time.monotonic()
        row = {'id': case_id}
        if browser:
            row['browser'] = browser
        try:
            row.update(status='pass', evidence=action())
        except Exception as error:
            row.update(status='fail', error=f'{type(error).__name__}: {error}')
            row['traceback'] = re.sub(r'BLOCKUNTU-(?:UNINSTALL-RECOVERY|TIER1-EDIT)(?:-[A-Z0-9]+)+',
                                      '[REDACTED TEST CREDENTIAL]', traceback.format_exc())
        row['seconds'] = round(time.monotonic() - start, 3)
        result['cases'].append(row)
        save_result(result)
        print(case_id, browser or '', row['status'], row.get('error', ''), flush=True)
        return row['status'] == 'pass'

    def units():
        data = {}
        names = ['blockuntu.socket', 'blockuntu.service', 'blockuntu-watchdog.service', 'blockuntu-hosts.path']
        def active():
            for unit in names:
                data[unit] = run('systemctl', 'show', unit, '-p', 'ActiveState', '-p', 'UnitFileState', '-p', 'NRestarts').stdout
            (WORK / 'unit-state.json').write_text(json.dumps(data, indent=2))
            return all('ActiveState=active' in data[unit] for unit in names)
        wait(active, 60)
        boot_log = run('sudo', '-n', 'journalctl', '-b', '--no-pager', '-o', 'cat').stdout
        cycles = [line for line in boot_log.splitlines() if 'ordering cycle' in line and 'blockuntu' in line]
        assert not cycles, 'BlocKuntu boot ordering cycle: ' + '; '.join(cycles)
        for unit in names:
            assert 'UnitFileState=enabled' in data[unit], data[unit]
            if 'NRestarts=' in data[unit]:
                assert 'NRestarts=0' in data[unit], data[unit]
        data['hosts-repair'] = run('systemctl', 'show', 'blockuntu-hosts.service', '-p', 'Result').stdout
        assert 'Result=success' in data['hosts-repair']
        data['rpc'] = rpc('status')
        assert data['rpc']['enforcement_state'] == 'active'
        time.sleep(10)
        for unit in ['blockuntu.service', 'blockuntu-watchdog.service']:
            assert run('systemctl', 'show', unit, '-p', 'NRestarts', '--value').stdout.strip() == '0'
        return data

    try:
        smoke_plan = json.loads((WORK / 'smoke-plan.json').read_text())
        case('VM-INSTALL-002', units)
        require_stage_passed(smoke_plan, result['cases'], 'services')
        prerequisite_commands = {
            'ubuntu': [
                ['sudo', '-n', 'apt-get', 'update'],
                ['sudo', '-n', 'env', 'DEBIAN_FRONTEND=noninteractive', 'apt-get',
                 '-o', 'DPkg::Lock::Timeout=180', 'install', '-y', 'python3-pyatspi', 'curl'],
            ],
            'fedora': [
                ['sudo', '-n', 'dnf', 'install', '-y', 'python3-pyatspi', 'curl', 'xz'],
            ],
            'cachyos': [
                ['sudo', '-n', 'pacman', '-S', '--noconfirm', '--needed',
                 'python-atspi', 'curl', 'xz'],
            ],
        }
        for command in prerequisite_commands[args.guest]:
            answer = run(*command, check=False, timeout=300)
            with (WORK / 'prerequisites.log').open('a') as log:
                log.write(answer.stdout + answer.stderr)
            if answer.returncode:
                raise RuntimeError('Guest prerequisites failed; see prerequisites.log')
        if p32_plan is not None and {row['id'] for row in p32_plan['observations']} == P32_DOMAIN_CASES:
            setup_domain_dns()
        configure_desktop(args.guest)
        run('systemd-run', '--user', '--collect', '--property=ExitType=cgroup',
            '--unit=blockuntu-acceptance-gui', 'gtk-launch', 'local.blockuntu.gui')
        from gui import run_cases
        try:
            run_cases(case, args.run_id, args.guest)
        except Exception as error:
            result['cases'].append({'id': 'SETUP-GUI', 'status': 'fail', 'error': str(error)})
            save_result(result)
            raise
        require_stage_passed(smoke_plan, result['cases'], 'gui')
        from browsers import run_cases as browser_cases
        try:
            browser_cases(case, result, args.run_id, args.guest,
                          p32_case=p32_case if p32_plan is not None and
                              any(row['browser_package'] for row in p32_plan['observations']) else None,
                          p32_ids={row['id'] for row in p32_plan['observations']}
                          if p32_plan is not None else None,
                          p32_browsers={row['browser_package'].removesuffix('-native')
                                        for row in p32_plan['observations'] if row['browser_package']} if p32_plan else None)
        except Exception as error:
            result['cases'].append({'id': 'SETUP-BROWSERS', 'status': 'blocked', 'reason': str(error)})
            save_result(result)
            raise
        require_stage_passed(smoke_plan, result['cases'], 'browsers')
        if p32_plan is not None:
            selected_ids = {row['id'] for row in p32_plan['observations']}
            if selected_ids == GUI_EDIT_CASES:
                from gui_edits import run_cases as edit_cases
                edit_cases(args.run_id, p32_case, p32_plan)
            elif selected_ids == HELPER_CASES:
                from helpers import run_cases as helper_cases
                helper_cases(args.run_id, p32_case)
        # Independent root supervisor keeps process evidence available if a user
        # session process is unexpectedly terminated by the tested allowlist.
        process_budget = process_budget_seconds(smoke_plan)
        answer = run('sudo', '-n', 'systemd-run', '--collect',
                     '--unit=blockuntu-process-acceptance',
                     '--property=RuntimeMaxSec=' + str(process_budget),
                     '/usr/bin/python3', str(WORK / 'processes.py'), '--run-id', args.run_id,
                     check=False, timeout=30)
        (WORK / 'process.log').write_text(answer.stdout + answer.stderr)
        if answer.returncode:
            raise RuntimeError(f'Process supervisor launch failed ({answer.returncode}): {answer.stderr[-2000:]}')
        # Do not keep a sudo/systemd-run wait process alive inside the user
        # allowlist experiment. The root supervisor publishes after cleanup.
        try:
            wait((WORK / 'process-completion.json').exists, process_budget + 30)
            completion = json.loads((WORK / 'process-completion.json').read_text())
            rows = process_rows(WORK / 'process-results.json')
            if completion.get('cases') != rows or completion.get('exit_code') not in (0, 1):
                raise ValueError('Process completion differs from its incremental checkpoint')
            result['cases'].extend(rows)
            save_result(result)
            if completion['exit_code']:
                result['cases'].append({'id': 'SETUP-PROCESSES', 'status': 'fail',
                                        'error': f"Process supervisor exited with {completion['exit_code']}"})
        except Exception:
            # A killed or timed-out supervisor may never publish completion.
            # Preserve each finished process observation before the outer setup
            # failure blocks the remaining cases and invalidates the suite.
            try:
                result['cases'].extend(process_rows(WORK / 'process-results.json'))
                save_result(result)
            except (OSError, ValueError, TypeError) as checkpoint_error:
                result['cases'].append({'id': 'SETUP-PROCESSES', 'status': 'fail',
                                        'error': f'Could not read process checkpoint: {checkpoint_error}'})
            raise
    except Exception as error:
        result['cases'].append({'id': 'SETUP-GUEST', 'status': 'fail', 'error': str(error)})
    finally:
        result['cases'].append({'id': 'VM-UPGRADE-001', 'status': 'skip', 'reason': 'Agreed upgrade deferral; no accepted baseline'})
        finalize_result(result, args.guest)
        save_result(result)
    return int(result['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
