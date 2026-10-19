"""Root-supervised Tier 3 allowlist journey; product RPC stays unprivileged."""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import pwd
import subprocess
import time

from guest import WORK, wait
from lifecycle import remaining, process_identity, finish


def exercise(run_id):
    assert os.geteuid() == 0, 'Independent supervisor must survive user allowlists'
    account = pwd.getpwnam('akhi')

    def drop_user():
        os.initgroups(account.pw_name, account.pw_gid)
        os.setgid(account.pw_gid)
        os.setuid(account.pw_uid)

    def user_rpc(method, params):
        code = ('import json,os,sys; from guest import rpc; assert os.geteuid()!=0; '
                'print(json.dumps(rpc(sys.argv[1],json.loads(sys.argv[2]))))')
        answer = subprocess.run(['/usr/bin/python3', '-c', code, method, json.dumps(params)],
            cwd=WORK, preexec_fn=drop_user, text=True, capture_output=True, timeout=15)
        if answer.returncode:
            raise RuntimeError(f'Ordinary-user {method} failed: {answer.stderr[-1500:]}')
        return json.loads(answer.stdout)

    def launch(name):
        p = subprocess.Popen([str(WORK / name), '--lifetime', '1800'], preexec_fn=drop_user,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        wait(lambda: p.poll() is not None or
             Path(f'/proc/{p.pid}/comm').read_text().strip() == name, 5)
        assert p.poll() is None, 'Fixture died before readiness'
        return p

    def hold(p, seconds, expected_remaining=None):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            assert p.poll() is None, 'Allowed/unlocked process died'
            if expected_remaining is not None:
                assert remaining(rule_id) == expected_remaining, 'Unexpected allowance consumption/refill'
            time.sleep(1)

    def killed(p):
        started = time.monotonic()
        wait(lambda: p.poll() is not None, 30)
        assert p.returncode in (0, -15, -9), p.returncode
        return time.monotonic() - started

    allowed = rejected = unlocked = None
    rule_id = run_id + '-metered-app'
    evidence = {'rule_id': rule_id, 'activation': 'targeted-detox'}
    try:
        allowed = launch('bk-test-app-a')
        paths = {str(WORK / 'bk-test-app-a'), str(Path('/usr/bin/python3').resolve())}
        # Record real executable paths individually, as in the existing smoke.
        # Do not give the denied fixture an inherited parent/directory exemption.
        from session_inventory import collect_processes
        paths.update(row['exe'] for row in collect_processes().values() if row['uid']==account.pw_uid)
        paths.discard(str(WORK / 'bk-test-app-b'))
        allowance_id = run_id + '-app-allowance'
        user_rpc('upsert_allowance', {'allowance': {'id': allowance_id, 'daily_minutes': 1}})
        user_rpc('upsert_app_rule', {'rule': {'id': rule_id, 'name': 'Tier 3 process lifecycle',
            'tier': 'controlled_access', 'mode': 'allowlist', 'allowance_id': allowance_id,
            'matchers': [{'kind': 'executable_path', 'value': p} for p in sorted(paths)]}})
        session = user_rpc('start_detox', {'name': 'Tier 3 process lifecycle',
            'duration_minutes': 5, 'app_rule_ids': [rule_id]})['session']
        wait(lambda: remaining(rule_id) is not None, 30)
        hold(allowed, 30, 60)
        evidence.update(allowed_only_uncharged=True, allowed_process=process_identity(allowed),
                        session=session, safety_paths=sorted(paths))
        rejected = launch('bk-test-app-b')
        evidence['rejected_process'] = process_identity(rejected)
        started = time.monotonic()
        positive = wait(lambda: value if isinstance(value := remaining(rule_id), int)
                        and 0 < value < 60 else None, 30)
        assert rejected.poll() is None, 'Process died before allowance exhaustion'
        wait(lambda: remaining(rule_id) == 0, max(1, 120 - (time.monotonic() - started)))
        evidence.update(positive_usage=True, remaining_after_positive_usage=positive,
                        exhaustion_seconds=time.monotonic() - started,
                        process_exit_seconds=killed(rejected))
        grant = user_rpc('request_unlock', {'target': rule_id,
            'reason': 'I need temporary access to finish this specific process acceptance check'})
        start = dt.datetime.fromisoformat(grant['started_at'].replace('Z', '+00:00'))
        expires = dt.datetime.fromisoformat(grant['expires_at'].replace('Z', '+00:00'))
        assert grant['rule_id'] == rule_id and (expires - start).total_seconds() == 120
        unlocked = launch('bk-test-app-b')
        evidence['unlocked_process'] = process_identity(unlocked)
        while (left := (expires - dt.datetime.now(dt.timezone.utc)).total_seconds()) > 0:
            assert unlocked.poll() is None, 'Unlocked process died before expiry'
            assert remaining(rule_id) == 0, 'Unlock refilled allowance'
            time.sleep(min(1, left))
        evidence['reblock_seconds'] = killed(unlocked)
        assert remaining(rule_id) == 0, 'Unlock refilled allowance'
        assert dt.datetime.now(dt.timezone.utc) < dt.datetime.fromisoformat(session['ends_at'].replace('Z', '+00:00'))
        assert allowed.poll() is None, 'Allowed control process died'
        evidence.update(grant=grant, grant_seconds=120, unlocked_process_survived=True,
            reblocked_after_expiry=True, remaining_after_exhaustion=0,
            enforcement_active=user_rpc('status', {})['enforcement_state'] == 'active')
        return evidence
    finally:
        for p in (allowed, rejected, unlocked):
            finish(p)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--run-id', required=True)
    args = parser.parse_args()
    row = {'status': 'running'}
    started = time.monotonic()
    try:
        row.update(status='pass', evidence=exercise(args.run_id))
    except Exception as error:
        row.update(status='fail', error=f'{type(error).__name__}: {error}')
    row['seconds'] = time.monotonic() - started
    temporary = WORK / 'application-lifecycle.tmp'
    temporary.write_text(json.dumps(row, indent=2) + '\n')
    temporary.replace(WORK / 'application-lifecycle.json')
    return int(row['status'] != 'pass')


if __name__ == '__main__':
    raise SystemExit(main())
