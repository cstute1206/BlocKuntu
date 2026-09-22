#!/usr/bin/env python3
"""Guest-side checks. Invoked only by the clone-guarded host runner."""
import argparse
import json
import os
import re
from pathlib import Path
import socket
import subprocess
import time
import traceback

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



def finalize_result(result):
    required = {(cid, None) for cid in (
        'VM-INSTALL-002', 'VM-INSTALL-003', 'VM-INSTALL-004', 'VM-INSTALL-005',
        'VM-DATA-001', 'VM-DATA-002', 'VM-DATA-003', 'VM-DATA-004',
        'VM-APP-001', 'VM-APP-002', 'VM-APP-003', 'VM-AAL-004')}
    required.update((cid, browser) for browser in ('firefox', 'chrome')
                    for cid in ('VM-BR-001', 'VM-BR-002', 'VM-WEB-002', 'VM-WAL-004'))
    observed = {(row['id'], row.get('browser')) for row in result['cases']}
    for cid, browser in sorted(required - observed, key=str):
        row = {'id': cid, 'status': 'blocked', 'reason': 'Prerequisite or earlier suite failure prevented execution'}
        if browser:
            row['browser'] = browser
        result['cases'].append(row)
    completed = {(row['id'], row.get('browser')) for row in result['cases'] if row['status'] == 'pass'}
    result['missing_required_cases'] = [cid + (':' + browser if browser else '')
                                      for cid, browser in sorted(required - completed, key=str)]
    result['status'] = ('fail' if any(row['status'] == 'fail' for row in result['cases']) else
                        'pass' if required <= completed else 'incomplete')

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--run-id', required=True)
    args = parser.parse_args()
    result = {'status': 'running', 'cases': []}
    save_result(result)
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
        case('VM-INSTALL-002', units)
        for command in [
            ['sudo', '-n', 'apt-get', 'update'],
            ['sudo', '-n', 'env', 'DEBIAN_FRONTEND=noninteractive', 'apt-get', '-o', 'DPkg::Lock::Timeout=180',
             'install', '-y', 'python3-pyatspi', 'curl'],
        ]:
            answer = run(*command, check=False, timeout=300)
            with (WORK / 'prerequisites.log').open('a') as log:
                log.write(answer.stdout + answer.stderr)
            if answer.returncode:
                raise RuntimeError('Guest prerequisites failed; see prerequisites.log')
        run('gsettings', 'set', 'org.gnome.desktop.interface', 'toolkit-accessibility', 'true')
        run('gsettings', 'set', 'org.gnome.desktop.session', 'idle-delay', '0')
        run('gsettings', 'set', 'org.gnome.desktop.screensaver', 'lock-enabled', 'false')
        run('gsettings', 'set', 'org.gnome.desktop.input-sources', 'sources', "[('xkb', 'us')]")
        run('systemd-run', '--user', '--collect', '--property=ExitType=cgroup',
            '--unit=blockuntu-acceptance-gui', 'gtk-launch', 'local.blockuntu.gui')
        from gui import run_cases
        try:
            run_cases(case, args.run_id)
        except Exception as error:
            result['cases'].append({'id': 'SETUP-GUI', 'status': 'fail', 'error': str(error)})
            save_result(result)
        from browsers import run_cases as browser_cases
        try:
            browser_cases(case, result, args.run_id)
        except Exception as error:
            result['cases'].append({'id': 'SETUP-BROWSERS', 'status': 'blocked', 'reason': str(error)})
            save_result(result)
        # Independent root supervisor keeps process evidence available if a user
        # session process is unexpectedly terminated by the tested allowlist.
        answer = run('sudo', '-n', 'systemd-run', '--collect',
                     '--unit=blockuntu-process-acceptance', '--property=RuntimeMaxSec=300',
                     '/usr/bin/python3', str(WORK / 'processes.py'), '--run-id', args.run_id,
                     check=False, timeout=30)
        (WORK / 'process.log').write_text(answer.stdout + answer.stderr)
        if answer.returncode:
            raise RuntimeError(f'Process supervisor launch failed ({answer.returncode}): {answer.stderr[-2000:]}')
        # Do not keep a sudo/systemd-run wait process alive inside the user
        # allowlist experiment. The root supervisor publishes after cleanup.
        wait((WORK / 'process-completion.json').exists, 330)
        completion = json.loads((WORK / 'process-completion.json').read_text())
        result['cases'].extend(completion['cases'])
        if completion['exit_code']:
            result['cases'].append({'id': 'SETUP-PROCESSES', 'status': 'fail',
                                    'error': f"Process supervisor exited with {completion['exit_code']}"})
    except Exception as error:
        result['cases'].append({'id': 'SETUP-GUEST', 'status': 'fail', 'error': str(error)})
    finally:
        result['cases'].append({'id': 'VM-UPGRADE-001', 'status': 'skip', 'reason': 'Agreed upgrade deferral; no accepted baseline'})
        finalize_result(result)
        save_result(result)
    return int(result['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
