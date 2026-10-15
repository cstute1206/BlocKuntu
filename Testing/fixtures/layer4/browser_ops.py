"""Clone-only fixed faults and independent, bounded recovery for P4.3."""
import argparse
from layer4_platform import detected_guest, package_state
import datetime as dt
import json
import os
from pathlib import Path
import pwd
import signal
import stat
import subprocess
import time

from layer4_browser_contract import MANIFESTS, POLICIES, SCENARIOS, save, fault_paths, TRANSIENT_PRE_PAUSE_SECONDS
from uninstall_ops import pause_automatic_packages

POLICIES, MANIFESTS = fault_paths(detected_guest())

WORK = Path('/home/akhi/Testing/layer3')
LEASE = Path('/run/blockuntu-p43-fault.json')


def run(*args, check=True):
    return subprocess.run(args, text=True, capture_output=True, timeout=30, check=check)


def publish(name, value):
    target = WORK / name
    save(target, value)
    account = pwd.getpwnam('akhi')
    os.chown(target, account.pw_uid, account.pw_gid)


def identity(pid):
    path = Path('/proc') / str(pid)
    stat = (path / 'stat').read_text().rsplit(')', 1)[1].split()
    return {'pid': int(pid), 'start_ticks': stat[19], 'state': stat[0],
            'exe': str((path / 'exe').resolve(strict=True)), 'uid': path.stat().st_uid}


def same_process(expected):
    try:
        actual = identity(expected['pid'])
        return all(actual[k] == expected[k] for k in ('pid','start_ticks','exe','uid'))
    except (OSError, ValueError):
        return False


def daemon():
    pid = int(run('systemctl','show','blockuntu.service','-p','MainPID','--value').stdout.strip())
    if pid <= 1:
        raise ValueError('No positive daemon MainPID')
    result = identity(pid)
    if result['exe'] != '/usr/bin/blockuntud' or result['uid'] != 0:
        raise ValueError('Unexpected packaged daemon identity')
    return result


def native_processes():
    answer = []
    for path in Path('/proc').iterdir():
        if not path.name.isdigit():
            continue
        try:
            record = identity(int(path.name))
            if record['exe'] == '/usr/bin/blockuntu-native' and record['uid'] == pwd.getpwnam('akhi').pw_uid:
                answer.append(record)
        except (OSError, ValueError):
            pass
    return sorted(answer, key=lambda row: row['pid'])


def inspect():
    def contents(path):
        target = Path(path)
        if target.is_symlink():
            raise ValueError('Refuse fault target symlink: ' + path)
        return target.read_text() if target.exists() else None
    return {'checked_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'monotonic': time.monotonic(),
        'files': {p: contents(p) for p in (*POLICIES, *MANIFESTS)},
        'daemon': daemon(), 'native_processes': native_processes(),
        'daemon_socket': {'exists': Path('/run/blockuntu/blockuntud.sock').exists(),
            'is_socket': Path('/run/blockuntu/blockuntud.sock').exists() and stat.S_ISSOCK(Path('/run/blockuntu/blockuntud.sock').stat().st_mode)},
        'units': run('systemctl','show','blockuntu.service','blockuntu.socket','blockuntu-watchdog.service',
                     '-p','Id','-p','ActiveState','-p','MainPID','-p','NRestarts', check=False).stdout,
        'collector_active': run('systemctl','is-active','blockuntu-p43-collector.service', check=False).stdout.strip() == 'active'}


def seed():
    jobs = pause_automatic_packages()
    hosts = Path('/etc/hosts')
    text = hosts.read_text()
    assert 'BEGIN BLOCKUNTU' not in text
    hosts.write_text(text.rstrip('\n') + '\n127.0.0.1 web.blockuntu.test\n')
    publish('p43-seeded.json', {'package_jobs': jobs, 'hosts': hosts.read_text()})


def write_target(path, content):
    if path not in (*POLICIES, *MANIFESTS) or Path(path).is_symlink():
        raise ValueError('Target outside fixed regular policy/manifest files')
    with Path(path).open('w') as stream:
        stream.write(content)
        stream.flush()
        os.fsync(stream.fileno())


def restore():
    if not LEASE.exists():
        return {'restored': True, 'nothing_to_restore': True}
    lease = json.loads(LEASE.read_text())
    if lease['scenario'] in ('outage','crash'):
        if lease['scenario'] == 'crash' and not same_process(lease['daemon']):
            LEASE.unlink()
            return {'restored': True, 'crash_already_restarted': True}
        if not same_process(lease['daemon']):
            raise ValueError('Paused daemon identity changed; refuse to signal reused PID')
        os.kill(lease['daemon']['pid'], signal.SIGCONT)
    elif lease['scenario'] == 'manifest':
        if set(lease['original_files']) != set(MANIFESTS):
            raise ValueError('Invalid recovery file set')
        for path, text in lease['original_files'].items():
            write_target(path, text)
    else:
        raise ValueError('Unknown recovery lease')
    result = {'restored': True, 'scenario': lease['scenario'], 'at_monotonic': time.monotonic(), 'inspection': inspect()}
    publish('p43-restored.json', result)
    LEASE.unlink()
    return result


def rescue():
    # Root-owned deadline survives ordinary-user suite/SSH/browser failures.
    lease = json.loads(LEASE.read_text())
    deadline = lease['deadline_monotonic']
    if deadline <= 0 or not 239 <= deadline - lease.get('armed_monotonic', deadline) <= 241:
        raise ValueError('Invalid bounded recovery deadline')
    while LEASE.exists() and time.monotonic() < deadline:
        time.sleep(1)
    if LEASE.exists():
        restore()


def fault(name):
    if name not in SCENARIOS:
        raise ValueError('Unknown fixed browser fault')
    before = inspect()
    if before['daemon']['state'] == 'T' or LEASE.exists():
        raise ValueError('Fault already active')
    witness = {'scenario': name, 'before': before, 'started_monotonic': time.monotonic()}
    if name in ('manifest', 'outage','crash'):
        armed = time.monotonic()
        lease = {'scenario': name, 'daemon': before['daemon'], 'armed_monotonic': armed, 'deadline_monotonic': armed + 240,
                 'original_files': {p: before['files'][p] for p in MANIFESTS} if name == 'manifest' else {}}
        save(LEASE, lease)
        try:
            run('systemd-run','--collect','--unit=blockuntu-p43-rescue','--property=RuntimeMaxSec=270',
                '/usr/bin/python3',str(WORK/'browser_ops.py'),'rescue')
        except Exception:
            LEASE.unlink()
            raise
        witness['automatic_restore_deadline'] = lease['deadline_monotonic']
    if name == 'policy':
        damaged = {}
        for path in POLICIES:
            value = json.loads(before['files'][path])
            if path == POLICIES[0]:
                value['policies']['ExtensionSettings'] = {}
            else:
                value['ExtensionInstallForcelist'] = []
            damaged[path] = json.dumps(value)
            write_target(path, damaged[path])
        witness['damaged_files'] = {p: Path(p).read_text() for p in POLICIES}
        witness['fault_witnessed'] = witness['damaged_files'] == damaged
    elif name == 'manifest':
        damaged = {}
        for path in MANIFESTS:
            value = json.loads(before['files'][path])
            value['path'] = '/nonexistent/blockuntu-p43-native'
            damaged[path] = json.dumps(value)
            write_target(path, damaged[path])
        witness['damaged_files'] = {p: Path(p).read_text() for p in MANIFESTS}
        witness['fault_witnessed'] = witness['damaged_files'] == damaged
        if len(before['native_processes']) < 2:
            raise ValueError('Both browser native connections must exist before breaking reconnect')
        for process in before['native_processes']:
            if not same_process(process):
                raise ValueError('Native process identity changed before termination')
            os.kill(process['pid'], signal.SIGKILL)
        deadline = time.monotonic() + 3
        while any(same_process(p) for p in before['native_processes']) and time.monotonic() < deadline:
            time.sleep(.05)
        witness['connections_terminated'] = all(not same_process(p) for p in before['native_processes'])
    else:
        process = before['daemon']
        if not same_process(process):
            raise ValueError('Daemon identity changed before fault')
        os.kill(process['pid'], signal.SIGSTOP)
        if name == 'crash':
            # Include a complete extension request cycle and its timeout, with
            # scheduling margin. Automatic restart remains enabled throughout.
            witness['transient_pre_pause_seconds'] = TRANSIENT_PRE_PAUSE_SECONDS
            paused_at = time.monotonic()
            time.sleep(TRANSIENT_PRE_PAUSE_SECONDS)
            witness['transient_pre_pause_elapsed_seconds'] = time.monotonic() - paused_at
            if not same_process(process):
                raise ValueError('Daemon identity changed during transient pre-pause')
            os.kill(process['pid'], signal.SIGKILL)
        if name == 'outage':
            for _ in range(20):
                if identity(process['pid'])['state'] == 'T':
                    break
                time.sleep(.05)
            witness['fault_witnessed'] = identity(process['pid'])['state'] == 'T'
            witness['restart_inhibition'] = 'SIGSTOP keeps service active; watchdog remains enabled; no unit masks/drop-ins'
        else:
            for _ in range(20):
                if not same_process(process):
                    break
                time.sleep(.05)
            witness['fault_witnessed'] = not same_process(process)
    publish('p43-fault.json', witness)
    return witness


def collect():
    history = WORK / 'p43-collector-history.jsonl'
    while True:
        try:
            state = inspect()
        except Exception as error:
            state = {'checked_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'error': str(error)}
        publish('p43-collector.json', state)
        with history.open('a') as stream:
            stream.write(json.dumps(state) + '\n')
        os.chmod(history, 0o600)
        account = pwd.getpwnam('akhi'); os.chown(history, account.pw_uid, account.pw_gid)
        time.sleep(2)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=('seed','inspect','collect','restore','rescue', *SCENARIOS))
    args = parser.parse_args()
    assert os.geteuid() == 0, 'Fixed root operations are invoked only by guarded clone runner'
    if args.action in SCENARIOS:
        print(json.dumps(fault(args.action)))
    elif args.action in ('inspect','restore'):
        print(json.dumps(globals()[args.action]()))
    else:
        globals()[args.action]()
