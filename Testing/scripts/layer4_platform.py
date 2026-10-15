"""Explicit native package/browser adapters shared by host and clone fixtures."""
from pathlib import Path
import subprocess
import json
import socket
import time

GUESTS = ('ubuntu', 'fedora', 'cachyos')


def detected_guest(path=Path('/etc/os-release')):
    fields = dict(line.split('=', 1) for line in path.read_text().splitlines() if '=' in line)
    identity = fields.get('ID', '').strip('"')
    guest = {'ubuntu': 'ubuntu', 'fedora': 'fedora', 'cachyos': 'cachyos', 'arch': 'cachyos'}.get(identity)
    if guest is None:
        raise ValueError('Unsupported Layer 4 guest: ' + identity)
    return guest


def browsers_for(guest):
    if guest not in GUESTS:
        raise ValueError('Unsupported Layer 4 guest')
    return ('firefox', 'chromium' if guest == 'cachyos' else 'chrome')


def browser_paths(guest):
    second = '/etc/chromium' if guest == 'cachyos' else '/etc/opt/chrome'
    browsers_for(guest)
    return (('/etc/firefox/policies/policies.json', second + '/policies/managed/blockuntu.json'),
            ('/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json',
             second + '/native-messaging-hosts/blockuntu_native.json'))


def removal_argv(guest):
    return {'ubuntu': ('/usr/bin/dpkg', '--purge', 'blockuntu'),
            'fedora': ('/usr/bin/dnf', 'remove', '--assumeyes', 'blockuntu'),
            'cachyos': ('/usr/bin/pacman', '-R', '--noconfirm', 'blockuntu')}[guest]


def direct_removal(guest, scenario):
    if scenario not in ('remove', 'purge'):
        raise ValueError('Unknown direct removal selection')
    return {'ubuntu': {'remove': 'DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=30 remove -y blockuntu',
                       'purge': 'DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=30 purge -y blockuntu'},
            'fedora': {'remove': 'dnf remove --assumeyes blockuntu', 'purge': 'rpm -e blockuntu'},
            'cachyos': {'remove': 'pacman -R --noconfirm blockuntu', 'purge': 'pacman -Rns --noconfirm blockuntu'}}[guest][scenario]


def prerequisites(guest, suite):
    package = {'ubuntu': {'browser': 'python3-pyatspi', 'hosts': 'dnsmasq-base'},
               'fedora': {'browser': 'python3-pyatspi', 'hosts': 'dnsmasq'},
               'cachyos': {'browser': 'python-atspi', 'hosts': 'dnsmasq'}}[guest][suite]
    return {'ubuntu': 'sudo -n apt-get update && sudo -n env DEBIAN_FRONTEND=noninteractive apt-get -o DPkg::Lock::Timeout=180 install -y ',
            'fedora': 'sudo -n dnf install -y ',
            'cachyos': 'sudo -n pacman -S --needed --noconfirm '}[guest] + package


def package_state(guest=None):
    guest = guest or detected_guest()
    args = {'ubuntu': ('dpkg-query', '-W', '-f=${db:Status-Status} ${Version}', 'blockuntu'),
            'fedora': ('rpm', '-q', '--qf', 'installed %{VERSION}-%{RELEASE}', 'blockuntu'),
            'cachyos': ('pacman', '-Q', 'blockuntu')}[guest]
    answer = subprocess.run(args, capture_output=True, text=True, timeout=10)
    output = answer.stdout
    if guest == 'cachyos' and answer.returncode == 0:
        fields = output.split()
        if len(fields) != 2 or fields[0] != 'blockuntu':
            raise ValueError('Unexpected native package identity')
        output = 'installed ' + fields[1]
    return {'returncode': answer.returncode, 'stdout': output, 'stderr': answer.stderr,
            'guest': guest, 'argv': list(args)}


def refusal_message(guest):
    return {'ubuntu': 'BlocKuntu refuses direct package-manager removal',
            'fedora': 'BlocKuntu refuses direct package-manager removal',
            'cachyos': 'BlocKuntu refuses direct Pacman removal'}[guest]


def verify_fixture_preparation(result, summary, directory, guest):
    refresh = summary.get('refresh_cachyos', False)
    if (type(refresh) is not bool or result.get('refresh_cachyos', False) != refresh or
            (refresh and guest != 'cachyos')):
        raise ValueError('Invalid or inconsistent rolling fixture preparation')
    if refresh:
        for name in ('cachyos-baseline.log', 'os-update.log',
                     'pacman-cache-before-update.log', 'pacman-cache-after-update.log'):
            path = Path(directory) / name
            if path.is_symlink() or not path.is_file() or not path.stat().st_size:
                raise ValueError('Rolling fixture preparation evidence missing: ' + name)
    return refresh


NSS_QUERY = """import json,socket,sys
try:
    rows=socket.getaddrinfo(sys.argv[1],None,socket.AF_INET,socket.SOCK_STREAM)
    proof={'addresses':sorted({row[4][0] for row in rows}),'errno':None,'error':None}
except socket.gaierror as error:
    proof={'addresses':[],'errno':error.errno,'error':str(error)}
print(json.dumps(proof))
"""


def resolution_witness(name, command):
    """Record utility diagnostics and bounded normal-NSS API results separately."""
    native = command('getent', 'ahostsv4', name, check=False, timeout=5)
    files = command('getent', '-s', 'files', 'ahostsv4', name, check=False, timeout=5)
    api = command('/usr/bin/python3', '-c', NSS_QUERY, name, check=False, timeout=5)
    if api.returncode != 0:
        raise RuntimeError('Bounded native NSS probe failed')
    def observation(answer):
        return {'returncode': answer.returncode,
                'addresses': sorted({line.split()[0] for line in answer.stdout.splitlines() if line.split()}),
                'stderr': answer.stderr}
    return {'native': observation(native), 'files': observation(files), 'nss': json.loads(api.stdout)}


def blocking_resolution(witness):
    """Require the exact hosts mapping and normal NSS zero-address or explicit denial."""
    native, files, nss = witness['native'], witness['files'], witness['nss']
    no_address = {socket.EAI_NONAME, getattr(socket, 'EAI_NODATA', socket.EAI_NONAME)}
    return (files['returncode'] == 0 and files['addresses'] == ['0.0.0.0'] and
            native['addresses'] in ([], ['0.0.0.0']) and
            ((nss['errno'] is None and nss['addresses'] == ['0.0.0.0']) or
             (nss['errno'] in no_address and nss['addresses'] == [])))


def hosts_mapping_resolution(witness, address):
    """A preserved hosts row must agree with both normal NSS interfaces."""
    if not isinstance(witness, dict):
        return False
    for key in ('native', 'files'):
        row = witness.get(key)
        if (not isinstance(row, dict) or type(row.get('returncode')) is not int or
                row['returncode'] != 0 or row.get('addresses') != [address]):
            return False
    nss = witness.get('nss')
    return (isinstance(nss, dict) and 'errno' in nss and nss['errno'] is None and
            nss.get('addresses') == [address])


def wait_hosts_mapping(name, address, command, waiter, seconds=30):
    """Observe native hosts-cache propagation without flushing or bypassing NSS."""
    samples = []
    started = time.monotonic()
    def ready():
        sample = resolution_witness(name, command)
        sample['elapsed_seconds'] = round(time.monotonic() - started, 3)
        samples.append(sample)
        return sample if hosts_mapping_resolution(sample, address) else None
    try:
        waiter(ready, seconds)
        if not samples or not hosts_mapping_resolution(samples[-1], address):
            raise TimeoutError('Waiter returned without a matching native lookup')
    except TimeoutError as error:
        raise TimeoutError('Normal NSS did not settle on ' + address + ' for ' +
                           name + ': ' + json.dumps(samples)) from error
    return {'expected_address': address, 'timeout_seconds': seconds,
            'settled': True, 'samples': samples}
