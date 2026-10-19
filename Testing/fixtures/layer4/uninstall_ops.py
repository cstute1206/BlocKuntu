"""Fixed guest-root setup and independent collector for native uninstall tests."""
import argparse
from layer4_platform import detected_guest, browser_paths, package_state, removal_argv
import datetime as dt
import fcntl
import gzip
import json
import os
from pathlib import Path
import pwd
import re
import subprocess
import sqlite3
from contextlib import closing
import time

from layer4_uninstall_contract import save

WORK = Path('/home/akhi/Testing/layer3')
HOST_SENTINEL = '127.0.0.42 p41-unrelated.blockuntu.test'
CHROME_SENTINEL = {'ShowHomeButton': True}
POLKIT = Path('/etc/polkit-1/rules.d/49-blockuntu-p41.rules')


def polkit_rule(env_program, guest='ubuntu'):
    """pkexec resolves env symlinks; still authorize only the leased purge argv."""
    programs = ['/usr/bin/env', str(env_program)]
    pattern = '^(' + '|'.join(re.escape(p) for p in programs) + \
              ') BLOCKUNTU_PACKAGE_REMOVAL_LEASE=[a-f0-9]{64} ' + ' '.join(re.escape(a) for a in removal_argv(guest)) + '$'
    return '''polkit.addRule(function(action, subject) {
  var programs = %s;
  var command = %s;
  if (action.id == "org.freedesktop.policykit.exec" && subject.user == "akhi" &&
      subject.local && subject.active && programs.indexOf(action.lookup("program")) >= 0 &&
      new RegExp(command).test(String(action.lookup("command_line")).trim())) {
    return polkit.Result.YES;
  }
});
''' % (json.dumps(programs), json.dumps(pattern))


def wait_package_locks(paths, seconds=300, poll=1):
    """Observe dpkg-style POSIX locks without deleting them or stopping owners."""
    deadline = time.monotonic() + seconds
    while True:
        descriptors = []
        try:
            for path in paths:
                fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o640)
                descriptors.append(fd)
                fcntl.lockf(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return
        except BlockingIOError:
            if time.monotonic() >= deadline:
                raise TimeoutError('In-flight package transaction did not finish')
        finally:
            for fd in descriptors:
                os.close(fd)
        time.sleep(poll)


def pause_automatic_packages():
    guest = detected_guest()
    if guest != 'ubuntu':
        timers = ('dnf-makecache.timer', 'dnf-automatic.timer') if guest == 'fedora' else ()
        # Mask background scheduling only; never interrupt an active transaction.
        present = [t for t in timers if subprocess.run(['systemctl', 'show', t, '-p', 'LoadState', '--value'], capture_output=True, text=True).stdout.strip() == 'loaded']
        if present:
            subprocess.run(['systemctl', 'mask', '--now', *present], check=True)
        if guest == 'cachyos':
            deadline = time.monotonic() + 300
            while Path('/var/lib/pacman/db.lck').exists():
                if time.monotonic() >= deadline:
                    raise TimeoutError('Native package lock did not finish')
                time.sleep(1)
        return {'guest': guest, 'masked_timers': present, 'transactions_not_interrupted': True}
    timers = ('apt-daily.timer', 'apt-daily-upgrade.timer')
    services = ('apt-daily.service', 'apt-daily-upgrade.service')
    subprocess.run(['systemctl', 'mask', '--now', *timers], check=True)
    # Mask services without stopping a transaction already in progress.
    subprocess.run(['systemctl', 'mask', *services], check=True)
    config = Path('/etc/apt/apt.conf.d/99-blockuntu-p41')
    config.write_text('APT::Periodic::Enable "0";\nAPT::Periodic::Unattended-Upgrade "0";\n')
    deadline = time.monotonic() + 300
    while any(s in ('active', 'activating') for s in subprocess.run(
            ['systemctl', 'is-active', *services], capture_output=True, text=True).stdout.split()):
        if time.monotonic() >= deadline:
            raise TimeoutError('Automatic package job did not finish')
        time.sleep(1)
    wait_package_locks(('/var/lib/dpkg/lock-frontend', '/var/lib/dpkg/lock', '/var/lib/apt/lists/lock'),
                       seconds=max(0, deadline - time.monotonic()))
    return {'masked_timers': list(timers), 'masked_services': list(services),
            'inflight_jobs_finished': True, 'package_locks_available': True}


def dpkg_logs(directory=Path('/var/log')):
    logs = []
    for path in sorted(directory.glob('dpkg.log*')):
        try:
            text = gzip.open(path, 'rt').read() if path.suffix == '.gz' else path.read_text()
            logs.append({'path': str(path), 'events': [line for line in text.splitlines() if ' blockuntu:' in line]})
        except (OSError, ValueError) as error:
            logs.append({'path': str(path), 'read_error': str(error)})
    return logs


def native_logs():
    guest = detected_guest()
    if guest == 'ubuntu':
        return dpkg_logs()
    paths = [Path('/var/log/pacman.log')] if guest == 'cachyos' else sorted(Path('/var/log').glob('dnf*.log*'))
    logs = []
    for path in paths:
        if path.is_file():
            content = gzip.open(path, 'rt').read() if path.suffix == '.gz' else path.read_text(errors='replace')
            logs.append({'path': str(path), 'events': [line for line in content.splitlines() if 'blockuntu' in line.lower()]})
    return logs


def browser_handoff(database=Path('/var/lib/blockuntu/blockuntu.sqlite3')):
    """Read only the daemon's actual transition timestamp; never create a DB."""
    if not database.is_file():
        return None
    try:
        with closing(sqlite3.connect(database.resolve().as_uri() + '?mode=ro', uri=True, timeout=1)) as connection:
            row = connection.execute("SELECT value,updated_at FROM service_state WHERE key='browser_extension_mode'").fetchone()
        return {'mode': row[0], 'started_at': row[1]} if row else None
    except sqlite3.Error:
        return None


def inspect():
    def command(*args):
        p = subprocess.run(args, capture_output=True, text=True, timeout=10)
        return {'returncode': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr}
    def file(path):
        p = Path(path)
        try:
            return p.read_text()
        except FileNotFoundError:
            return None
    guest = detected_guest()
    inventory = None
    if guest in ('fedora', 'cachyos'):
        argv = ('rpm', '-qa', '--qf', '%{NAME} %{EPOCHNUM}:%{VERSION}-%{RELEASE}.%{ARCH}\n') if guest == 'fedora' else ('pacman', '-Q')
        answer = command(*argv)
        inventory = {'guest': guest, 'returncode': answer['returncode'],
                     'packages': sorted(answer['stdout'].splitlines())}
    policies, _ = browser_paths(guest)
    paths = (policies[0], policies[1].replace('blockuntu.json', 'p41-unrelated.json'), policies[1],
             '/usr/bin/blockuntud', '/usr/bin/blockuntu-native',
             '/usr/bin/blockuntu-setup-confined-chromium', '/usr/bin/blockuntu-setup-confined-firefox',
             '/usr/bin/blockuntu-gui', '/etc/blockuntu', '/var/lib/blockuntu',
             '/run/blockuntu', '/run/blockuntu/package-removal-lease')
    paths += tuple(root + '/policies/managed/blockuntu.json' for root in
                   ('/etc/opt/chrome', '/etc/chromium', '/etc/brave', '/etc/opt/opera',
                    '/etc/opt/edge', '/etc/vivaldi', '/etc/opt/vivaldi'))
    if guest == 'fedora':
        paths += ('/usr/libexec/blockuntu/check-package-removal-lease.sh',
                  '/etc/dnf/libdnf5-plugins/actions.d/blockuntu-removal.actions')
    elif guest == 'cachyos':
        paths += ('/usr/lib/blockuntu/check-package-removal-lease.sh',
                  '/usr/share/libalpm/hooks/00-blockuntu-removal.hook')
    return {'checked_at': dt.datetime.now(dt.timezone.utc).isoformat(),
            'package': package_state(), 'package_inventory': inventory, 'browser_handoff': browser_handoff(),
            'hosts': file('/etc/hosts'), 'hosts_attributes': command('lsattr', '/etc/hosts'),
            'firefox_policy': file(paths[0]), 'chromium_sentinel': file(paths[1]),
            'exists': {p: Path(p).exists() for p in paths},
            'native_manifests': sorted({str(p) for root in ('/etc', '/usr/lib', '/usr/lib64', '/home/akhi')
                                 for pattern in ('**/blockuntu_native.json', '**/local.blockuntu*.json')
                                 for p in Path(root).glob(pattern)}),
            'user_native_copies': sorted(str(p) for p in Path('/home/akhi').glob('**/blockuntu-native*') if p.is_file()),
            'units': command('systemctl', 'show', 'blockuntu.service', 'blockuntu.socket',
                             'blockuntu-watchdog.service', 'blockuntu-hosts.path', 'blockuntu-hosts.service',
                             '-p', 'Id', '-p', 'LoadState', '-p', 'ActiveState'),
            'collector': command('systemctl', 'is-active', 'blockuntu-p41-collector.service')}


def seed():
    baseline = {'hosts': Path('/etc/hosts').read_text(), 'package_job_fixture': pause_automatic_packages()}
    assert 'BEGIN BLOCKUNTU' not in baseline['hosts'], 'Seed requires pristine clone'
    Path('/etc/hosts').write_text(baseline['hosts'].rstrip('\n') + '\n' + HOST_SENTINEL + '\n'
                                '127.0.0.1 web.blockuntu.test\n')
    firefox = Path('/etc/firefox/policies/policies.json')
    firefox.parent.mkdir(parents=True, exist_ok=True)
    policies = json.loads(firefox.read_text()) if firefox.exists() else {'policies': {}}
    baseline['firefox_policy'] = policies
    policies.setdefault('policies', {})['DisableTelemetry'] = True
    firefox.write_text(json.dumps(policies, indent=2) + '\n')
    chrome = Path(browser_paths(detected_guest())[0][1].replace('blockuntu.json', 'p41-unrelated.json'))
    chrome.parent.mkdir(parents=True, exist_ok=True)
    assert not chrome.exists(), 'Sentinel must be fresh'
    chrome.write_text(json.dumps(CHROME_SENTINEL) + '\n')
    # Exercise actual GUI -> pkexec -> env -> native package manager, without a personal password.
    # This clone-only OS authorization fixture does not create a removal lease.
    env_program = Path('/usr/bin/env').resolve(strict=True)
    POLKIT.write_text(polkit_rule(env_program, detected_guest()))
    baseline['polkit_env_program'] = str(env_program)
    save(WORK / 'p41-seeded.json', baseline)
    account = pwd.getpwnam('akhi')
    os.chown(WORK / 'p41-seeded.json', account.pw_uid, account.pw_gid)


def collect():
    while True:
        state = inspect()
        save(WORK / 'p41-collector.json', state)
        account = pwd.getpwnam('akhi')
        os.chown(WORK / 'p41-collector.json', account.pw_uid, account.pw_gid)
        print(json.dumps({'checked_at': state['checked_at'], 'package': state['package'],
                          'hosts_attributes': state['hosts_attributes']}), flush=True)
        time.sleep(5)


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('action', choices=('seed', 'collect', 'inspect', 'dpkg-log', 'native-log', 'handoff'))
    args = p.parse_args()
    assert os.geteuid() == 0, 'Only the guarded clone runner invokes root operations'
    if args.action == 'seed':
        seed()
    elif args.action == 'collect':
        collect()
    elif args.action in ('dpkg-log', 'native-log'):
        print(json.dumps(native_logs()))
    elif args.action == 'handoff':
        print(json.dumps(browser_handoff()))
    else:
        print(json.dumps(inspect()))
