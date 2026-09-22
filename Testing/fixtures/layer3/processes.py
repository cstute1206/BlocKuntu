#!/usr/bin/env python3
"""Observe real process enforcement; root supervisor survives user allowlists."""
import argparse
import json
import os
from pathlib import Path
import pwd
import signal
import subprocess
import time
from guest import WORK, rpc, wait


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--run-id', required=True)
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit('Process suite requires guest root supervisor')
    account = pwd.getpwnam('akhi')
    children = []
    results = []
    prefix = args.run_id + '-process'

    def drop_user():
        os.initgroups(account.pw_name, account.pw_gid)
        os.setgid(account.pw_gid)
        os.setuid(account.pw_uid)

    def launch(name):
        process = subprocess.Popen([str(WORK / name), '--lifetime', '600'], preexec_fn=drop_user,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        children.append(process)
        wait(lambda: Path(f'/proc/{process.pid}/exe').readlink().name == name and
             Path(f'/proc/{process.pid}/comm').read_text().strip() == name, 5)
        return process

    def identity(process):
        return {'pid': process.pid, 'comm': Path(f'/proc/{process.pid}/comm').read_text().strip(),
                'exe': str(Path(f'/proc/{process.pid}/exe').readlink()),
                'start_ticks': Path(f'/proc/{process.pid}/stat').read_text().rsplit(')', 1)[1].split()[19]}

    def terminated(process):
        wait(lambda: process.poll() is not None, 30)
        # The fixture lives for 600 seconds and exits cleanly on SIGTERM.
        assert process.returncode in (0, -signal.SIGTERM, -signal.SIGKILL), process.returncode

    def alive(process, seconds=30):
        start = time.monotonic()
        while time.monotonic() - start < seconds:
            assert process.poll() is None, 'Allowed/near-miss process was terminated'
            time.sleep(1)

    def case(case_id, action):
        start = time.monotonic()
        try:
            evidence = action()
            results.append({'id': case_id, 'status': 'pass', 'evidence': evidence,
                            'seconds': round(time.monotonic() - start, 3)})
        except Exception as error:
            results.append({'id': case_id, 'status': 'fail', 'error': f'{type(error).__name__}: {error}'})
        (WORK / 'process-results.json').write_text(json.dumps(results, indent=2))
        print(case_id, results[-1]['status'], flush=True)

    def block_existing():
        process = launch('bk-test-block')
        info = identity(process)
        rpc('upsert_app_rule', {'rule': {'id': prefix + '-block', 'name': 'Acceptance block fixture',
            'tier': 'hard', 'matchers': [{'kind': 'command_name', 'value': 'bk-test-block'}]}})
        terminated(process)
        return info

    def block_new():
        process = launch('bk-test-block')
        info = identity(process)
        terminated(process)
        return info

    def near_miss():
        # Both identities share bk-test- and the executable bk-test- prefix.
        process = launch('bk-test-app-b')
        info = identity(process)
        alive(process)
        process.terminate(); process.wait(timeout=5)
        return info

    def allowlist():
        a = launch('bk-test-app-a')
        b = launch('bk-test-app-b')
        evidence = {'allowed': identity(a), 'rejected': identity(b)}
        # Curate the currently present desktop identities individually. Exclude B;
        # permission is never inherited from a parent or a directory prefix.
        excluded = {str(WORK / 'bk-test-app-b'), str(WORK / 'bk-test-helper')}
        paths = set()
        for entry in Path('/proc').iterdir():
            if not entry.name.isdigit():
                continue
            try:
                if entry.stat().st_uid == account.pw_uid:
                    target = str((entry / 'exe').readlink())
                    if target not in excluded:
                        paths.add(target)
            except (FileNotFoundError, PermissionError, ProcessLookupError):
                pass
        evidence['safety_paths'] = sorted(paths)
        rpc('upsert_app_rule', {'rule': {'id': prefix + '-allow', 'name': 'Acceptance allowlist fixture',
            'tier': 'scheduled_block', 'mode': 'allowlist',
            'matchers': [{'kind': 'executable_path', 'value': p} for p in sorted(paths)]}})
        # Inactive lists must not kill either fixture.
        alive(a, 15); alive(b, 15)
        session = rpc('start_detox', {'name': 'Acceptance process allowlist', 'duration_minutes': 3,
                                     'app_rule_ids': [prefix + '-allow']})
        evidence['detox'] = session
        terminated(b)
        alive(a, 30)
        return evidence

    try:
        case('VM-APP-001', block_existing)
        case('VM-APP-002', block_new)
        case('VM-APP-003', near_miss)
        case('VM-AAL-004', allowlist)
    finally:
        for process in children:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait(timeout=5)
    exit_code = int(any(row['status'] == 'fail' for row in results))
    completion = WORK / 'process-completion.tmp'
    completion.write_text(json.dumps({'exit_code': exit_code, 'cases': results}))
    completion.replace(WORK / 'process-completion.json')
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
