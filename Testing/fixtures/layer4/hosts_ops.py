"""Fixed /etc/hosts faults and independent root collection in guarded P4.2 clones."""
import argparse
from layer4_platform import browser_paths, detected_guest, package_state
import datetime as dt
import ipaddress
import shutil
import json
import os
from pathlib import Path
import pwd
import subprocess
import time

from layer4_hosts_contract import (APPEND_SENTINEL, BASE_SENTINEL, FAULTS,
                                   mutated_hosts, save)
from uninstall_ops import pause_automatic_packages

WORK = Path('/home/akhi/Testing/layer3')
HOSTS = Path('/etc/hosts')


def run(*args, check=True):
    return subprocess.run(args, capture_output=True, text=True, check=check, timeout=30)


def publish(name, data):
    path = WORK / name
    save(path, data)
    account = pwd.getpwnam('akhi')
    os.chown(path, account.pw_uid, account.pw_gid)


def inspect():
    def command(*args):
        answer = run(*args, check=False)
        return {'returncode': answer.returncode, 'stdout': answer.stdout, 'stderr': answer.stderr}
    try:
        text = HOSTS.read_text()
    except FileNotFoundError:
        text = None
    return {'checked_at': dt.datetime.now(dt.timezone.utc).isoformat(), 'hosts': text,
            'hosts_attributes': command('lsattr', '/etc/hosts'),
            'package': package_state(),
            'units': command('systemctl', 'show', 'blockuntu.service', 'blockuntu-hosts.path', 'blockuntu-hosts.service',
                             '-p', 'Id', '-p', 'LoadState', '-p', 'ActiveState', '-p', 'MainPID'),
            'collector': command('systemctl', 'is-active', 'blockuntu-p42-collector.service')}


def seed():
    text = HOSTS.read_text()
    assert 'BEGIN BLOCKUNTU' not in text, 'Seed requires pristine clone'
    jobs = pause_automatic_packages()
    HOSTS.write_text(text.rstrip('\n') + '\n' + BASE_SENTINEL + '\n')
    publish('p42-seeded.json', {'original_hosts': text, 'seeded_hosts': HOSTS.read_text(), 'package_job_fixture': jobs})


def setup():
    lines = run('ip', '-o', '-4', 'route', 'show', 'default').stdout.splitlines()
    assert len(lines) == 1, 'One default guest link required'
    tokens = lines[0].split()
    link = tokens[tokens.index('dev') + 1]
    current = run('resolvectl', 'dns', link).stdout.split(':', 1)[1].split()
    upstream = next(address for address in current if ipaddress.ip_address(address).version == 4 and not ipaddress.ip_address(address).is_loopback)
    run('systemd-run', '--collect', '--unit=blockuntu-p42-dns',
        shutil.which('dnsmasq'), '--no-daemon', '--no-resolv', '--server=' + upstream,
        '--listen-address=127.0.0.1', '--bind-interfaces', '--port=53',
        '--address=/p42.blockuntu.test/127.0.0.2')
    run('resolvectl', 'dns', link, '127.0.0.1')
    run('resolvectl', 'flush-caches')
    run('systemd-run', '--collect', '--unit=blockuntu-p42-site', '/usr/bin/python3',
        str(WORK / 'server.py'), '--bind', '127.0.0.2', '--port', '18080')
    publish('p42-network.json', {'link': link, 'upstream': upstream, 'fixture_address': '127.0.0.2',
                               'domain': 'p42.blockuntu.test', 'http_port': 18080})


def fault(name):
    if name not in FAULTS.values():
        raise ValueError('Unknown fixed hosts fault')
    before = inspect()
    if not before['hosts']:
        raise ValueError('Fault requires an existing baseline hosts file')
    target = None if name == 'delete-hosts' else mutated_hosts(before['hosts'], name)
    # A running repair can restore immutability between clear and write. Retry
    # only this named fault; never stop the daemon or its repair/watchdog units.
    for attempt in range(3):
        run('chattr', '-i', '/etc/hosts')
        cleared = run('lsattr', '/etc/hosts').stdout.split()[0]
        try:
            if target is None:
                HOSTS.unlink()
                observed = None if not HOSTS.exists() else HOSTS.read_text()
            else:
                with HOSTS.open('w') as stream:
                    stream.write(target)
                    stream.flush()
                    os.fsync(stream.fileno())
                observed = HOSTS.read_text()
            witnessed = observed == target
            evidence = {'name': name, 'checked_at': dt.datetime.now(dt.timezone.utc).isoformat(),
                        'before': before, 'immutable_clear_witnessed': 'i' not in cleared,
                        'fault_witnessed': witnessed, 'fault_hosts': observed, 'attempt': attempt + 1,
                        'after': inspect()}
            publish('p42-fault.json', evidence)
            if not witnessed:
                raise RuntimeError('Repair raced the fault witness; no damaged state observed')
            return evidence
        except PermissionError:
            if attempt == 2:
                raise
    raise RuntimeError('Named fault could not be applied')


def collect():
    while True:
        state = inspect()
        publish('p42-collector.json', state)
        print(json.dumps({'checked_at': state['checked_at'], 'hosts_attributes': state['hosts_attributes']}), flush=True)
        time.sleep(2)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=('seed', 'setup', 'inspect', 'collect', *FAULTS.values()))
    args = parser.parse_args()
    assert os.geteuid() == 0, 'Root operations are invoked only by the guarded clone runner'
    if args.action in FAULTS.values():
        print(json.dumps(fault(args.action)))
    elif args.action == 'inspect':
        print(json.dumps(inspect()))
    else:
        globals()[args.action]()
