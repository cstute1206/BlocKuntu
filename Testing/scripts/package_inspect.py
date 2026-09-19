"""Inspect package archives without installing them or executing their hooks."""
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import posixpath
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = json.loads((ROOT / 'Testing/package-ci/contract.json').read_text())


def require(condition, message):
    if not condition:
        raise ValueError(message)


def output(command):
    return subprocess.check_output(command, stderr=subprocess.PIPE)


def sha256(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def archive_entries(stream):
    """Read only; reject paths/links that could escape an extraction directory."""
    entries = {}
    with tarfile.open(fileobj=stream, mode='r|*') as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and '..' not in path.parts, f'Unsafe path: {path}')
            name = str(path)
            if name == '.':
                continue
            require(name not in entries, f'Duplicate archive member: {name}')
            link = None
            if member.issym():
                # RPM generates build-id links; allow only these exact in-package targets.
                link = posixpath.normpath(posixpath.join(str(path.parent), member.linkname))
                require(name.startswith('usr/lib/.build-id/') and not member.linkname.startswith('/')
                        and link in {'usr/bin/blockuntud', 'usr/bin/blockuntu-native', 'usr/bin/blockuntu-gui'},
                        f'Unexpected/unsafe symlink: {name}')
            require(member.isfile() or member.isdir() or link, f'Unexpected link/device: {name}')
            data = b''
            if member.isfile():
                # ELF validation needs only its header. Textual contracts are small.
                limit = 64 if name in {'usr/bin/blockuntud', 'usr/bin/blockuntu-native', 'usr/bin/blockuntu-gui'} else 2_000_000
                require(member.size <= limit or limit == 64, f'Oversized non-binary member: {name}')
                data = archive.extractfile(member).read(limit)
            entries[name] = {'data': data, 'mode': member.mode, 'uid': member.uid,
                             'gid': member.gid, 'file': member.isfile(), 'size': member.size, 'link': link}
    for name, value in entries.items():
        if value['link']:
            require(value['link'] in entries and entries[value['link']]['file'], f'Dangling build-id link: {name}')
    return entries


def read_package(path, kind):
    path = Path(path).resolve()
    with tempfile.TemporaryFile() as stream:
        command = (['dpkg-deb', '--fsys-tarfile', str(path)] if kind == 'deb'
                   else ['bsdtar', '-cf', '-', '--format=pax', '@' + str(path)])
        subprocess.run(command, stdout=stream, check=True)
        stream.seek(0)
        entries = archive_entries(stream)
    if kind == 'deb':
        fields = ['Package', 'Version', 'Architecture', 'Depends', 'Description']
        meta = {field.lower(): output(['dpkg-deb', '-f', str(path), field]).decode().strip() for field in fields}
        meta['name'] = meta.pop('package')
        meta['dependencies'] = re.findall(r'(?:^|,)\s*([a-z0-9+.-]+)', meta.pop('depends'))
        control = archive_entries(io.BytesIO(output(['dpkg-deb', '--ctrl-tarfile', str(path)])))
        hooks = {name: control[name]['data'].decode() for name in ['postinst', 'prerm', 'postrm'] if name in control}
        for name in hooks:
            require(control[name]['mode'] == 0o755, f'Wrong hook mode: {name}')
    elif kind == 'rpm':
        with tempfile.TemporaryDirectory() as db:
            base = ['rpm', '--dbpath', db, '-qp']
            values = output(base + ['--qf', '%{NAME}\n%{VERSION}-%{RELEASE}\n%{ARCH}\n%{LICENSE}\n%{SUMMARY}\n[%{REQUIRENAME}\n]', str(path)]).decode().splitlines()
            meta = dict(zip(['name', 'version', 'architecture', 'license', 'description'], values[:5]))
            meta['dependencies'] = values[5:]
            hooks = {key: output(base + ['--qf', '%{' + tag + '}', str(path)]).decode()
                     for key, tag in [('preinst', 'PREIN'), ('postinst', 'POSTIN'), ('prerm', 'PREUN'), ('postrm', 'POSTUN'), ('posttrans', 'POSTTRANS')]}
            hooks = {key: value for key, value in hooks.items() if value.strip() != '(none)'}
    else:
        fields = {}
        for line in entries['.PKGINFO']['data'].decode().splitlines():
            if ' = ' in line:
                key, value = line.split(' = ', 1)
                fields.setdefault(key, []).append(value)
        meta = {key: fields[source][0] for key, source in [('name', 'pkgname'), ('version', 'pkgver'), ('architecture', 'arch'), ('license', 'license'), ('description', 'pkgdesc')]}
        meta['dependencies'] = [re.split(r'[<>=]', value)[0] for value in fields.get('depend', [])]
        if '.BUILDINFO' in entries:
            meta['build_info'] = entries['.BUILDINFO']['data'].decode()
        hooks = {'install': entries['.INSTALL']['data'].decode()}
    return entries, meta, hooks


def inspect_package(entries, meta, hooks, kind):
    results = []
    target = CONTRACT['targets'][kind]

    def case(identifier, function):
        try:
            function()
            results.append({'id': identifier, 'status': 'pass'})
        except (ValueError, KeyError, UnicodeError, TypeError) as error:
            results.append({'id': identifier, 'status': 'fail', 'reason': str(error)})

    def entry(path, mode=0o644):
        value = entries[path]
        require(value['file'] and value['mode'] == mode, f'{path}: expected regular file with mode {mode:o}')
        return value['data']

    def text(path, mode=0o644):
        return entry(path, mode).decode()

    def version():
        expected = CONTRACT['version'] + '-' + CONTRACT['release']
        require(meta['version'] == expected or (kind == 'rpm' and re.fullmatch(re.escape(expected) + r'\.fc\d+', meta['version'])), f'Unexpected package version: {meta["version"]}; expected {expected}')
    case('PKG-META-001', version)

    def metadata():
        require(meta['name'] == 'blockuntu', 'Wrong package name')
        require(meta['architecture'] == target['architecture'], 'Wrong architecture')
        require(bool(meta['description'].strip()), 'Missing description')
        require(set(target['dependencies']) <= set(meta['dependencies']), 'Missing required dependencies')
        if kind != 'deb':
            require(meta['license'] == 'MIT', 'Expected MIT license')
        license_paths = [p for p in entries if p == 'usr/share/doc/blockuntu/copyright' or (p.startswith('usr/share/licenses/blockuntu') and p.endswith('/LICENSE'))]
        require(bool(license_paths), 'Missing packaged license')
        require(any('MIT License' in text(p) for p in license_paths), 'Invalid packaged MIT license')
    case('PKG-META-002', metadata)

    def executables():
        for name in ['blockuntud', 'blockuntu-native', 'blockuntu-gui']:
            header = entry('usr/bin/' + name, 0o755)
            require(header[:6] == b'\x7fELF\x02\x01' and header[18:20] == b'\x3e\x00', f'{name}: expected x86-64 ELF')
    case('PKG-CONTENT-001', executables)
    units = target['unit_dir']

    def systemd():
        for name in ['blockuntu.socket', 'blockuntu.service', 'blockuntu-watchdog.service', 'blockuntu-hosts.path', 'blockuntu-hosts.service']:
            require('[Unit]' in text(units + '/' + name), f'Invalid unit {name}')
        service = text(units + '/blockuntu.service')
        require('ExecStart=/usr/bin/blockuntud --snap-native-bridge --defer-browser-policy-repair-until-heartbeat serve' in service, 'Wrong daemon command')
        require('ExecStart=/usr/bin/blockuntud repair-hosts' in text(units + '/blockuntu-hosts.service'), 'Wrong hosts repair executable')
        socket = text(units + '/blockuntu.socket')
        require('SocketMode=0660' in socket and 'SocketGroup=blockuntu' in socket, 'Wrong socket permissions')
        require('StateDirectoryMode=0700' in service, 'Unsafe state directory mode')
    case('PKG-CONTENT-002', systemd)

    def defaults():
        config = tomllib.loads(text('etc/blockuntu/config.toml'))
        strict = config['strict_mode']
        for key in ['require_firefox_extension', 'require_chrome_extension', 'kill_supported_browser_if_extension_stale', 'block_unsupported_browsers']:
            require(strict[key] is True, f'Unsafe default: {key}')
        require(strict['grace_seconds'] == 30, 'Unexpected grace period')
    case('PKG-CONTENT-003', defaults)

    def manifests():
        paths = [(target['mozilla_lib'] + '/' + browser + '/native-messaging-hosts/blockuntu_native.json', 'allowed_extensions', CONTRACT['firefox_ids']) for browser in ['mozilla', 'librewolf', 'waterfox']]
        paths += [(base + '/native-messaging-hosts/blockuntu_native.json', 'allowed_origins', CONTRACT['chrome_origins']) for base in ['etc/opt/chrome', 'etc/chromium', 'etc/opt/edge', 'etc/opt/vivaldi', 'etc/vivaldi']]
        for path, key, ids in paths:
            manifest = json.loads(text(path))
            require(manifest['name'] == 'blockuntu_native' and manifest['type'] == 'stdio', f'Invalid native host: {path}')
            require(manifest['path'] == '/usr/bin/blockuntu-native', f'Wrong native executable: {path}')
            require(manifest[key] == ids, f'Wrong browser identities: {path}')
    case('PKG-CONTENT-004', manifests)
    all_hooks = '\n'.join(hooks.values())

    def policy_paths():
        # Policies are generated at runtime. Here verify packaged lifecycle references;
        # actual browser discovery and policy writing remain VM acceptance.
        for base in ['etc/firefox/policies/policies.json', 'etc/opt/chrome', 'etc/chromium', 'etc/brave', 'etc/opt/opera', 'etc/opt/edge', 'etc/vivaldi']:
            require('/' + base in all_hooks, f'Missing browser policy lifecycle path: {base}')
    case('PKG-CONTENT-005', policy_paths)

    def helpers():
        for browser in ['firefox', 'chromium']:
            path = target['helper_dir'] + '/setup-confined-' + browser + '-native-host.sh'
            require(entry(path, 0o755).startswith(b'#!'), f'Invalid helper {path}')
            require('exec /' + path in text('usr/bin/blockuntu-setup-confined-' + browser, 0o755), 'Wrong helper wrapper')
        require('create_snap_native_bridge_token' in all_hooks, 'Missing bridge token creation')
    case('PKG-CONTENT-006', helpers)

    def desktop():
        data = text('usr/share/applications/local.blockuntu.gui.desktop')
        require('Exec=/usr/bin/blockuntu-gui\n' in data and 'Icon=blockuntu-gui\n' in data, 'Wrong desktop entry')
        for size in [32, 64, 128]:
            for name in ['blockuntu', 'blockuntu-gui']:
                require(entry(f'usr/share/icons/hicolor/{size}x{size}/apps/{name}.png').startswith(b'\x89PNG\r\n\x1a\n'), 'Invalid icon')
    case('PKG-CONTENT-007', desktop)

    def lifecycle():
        required = ['install'] if kind == 'arch' else ['postinst', 'prerm', 'postrm']
        require(all(hooks.get(key, '').strip() not in ['', '(none)'] for key in required), 'Missing lifecycle hooks')
        for fragment in ['systemctl daemon-reload', 'systemctl', 'remove_hosts_block', 'remove_browser_policies', 'recovery-credentials-hidden']:
            require(fragment in all_hooks, f'Missing hook contract: {fragment}')
        for name, body in hooks.items():
            check = subprocess.run(['bash' if kind == 'arch' else 'sh', '-n'], input=body.encode(), capture_output=True)
            require(check.returncode == 0, f'Invalid {name} syntax: {check.stderr.decode()}')
    case('PKG-CONTENT-008', lifecycle)

    def permissions():
        for path, value in entries.items():
            require(value['uid'] == 0 and value['gid'] == 0, f'Non-root ownership: {path}')
            if not value.get('link'):
                require(not value['mode'] & 0o7022, f'Unsafe permissions: {path}')
        for name in ['uninstall-recovery.txt', 'tier1-edit-key.txt', 'snap-native-bridge-token', 'policy-recovery.toml']:
            require('etc/blockuntu/' + name not in entries, f'Runtime secret/policy must not be shipped: {name}')
    case('PKG-CONTENT-009', permissions)

    def development_paths():
        for path, value in entries.items():
            if path.startswith(units + '/') or '/native-messaging-hosts/' in path or path.endswith('.desktop'):
                data = value['data'].decode()
                require(not any(word in data for word in ['/usr/local/', '/target/', '/home/', '/tmp/']), f'Development path: {path}')
    case('PKG-CONTENT-010', development_paths)

    def removal_contract():
        for fragment in ['reject_package_uninstall', 'package-removal-lease', 'BLOCKUNTU_PACKAGE_REMOVAL_LEASE', 'expires_at']:
            require(fragment in all_hooks, f'Missing removal contract: {fragment}')
    case('PKG-HARD-001', removal_contract)

    def recovery_units():
        require('systemctl enable --now blockuntu.socket blockuntu.service blockuntu-watchdog.service blockuntu-hosts.path' in all_hooks, 'Recovery units not enabled by hook contract')
        require('Restart=always' in text(units + '/blockuntu-watchdog.service'), 'Missing watchdog restart')
        require('PathChanged=/etc/hosts' in text(units + '/blockuntu-hosts.path'), 'Missing hosts watch')
    case('PKG-HARD-002', recovery_units)

    def credentials():
        for fragment in ['policy-recovery.toml', 'uninstall-recovery.txt', 'tier1-edit-key.txt', 'snap-native-bridge-token', 'install -o root -g blockuntu -m 0640']:
            require(fragment in all_hooks, f'Missing recovery-file contract: {fragment}')
    case('PKG-HARD-003', credentials)

    def no_allowlist():
        config = tomllib.loads(text('etc/blockuntu/config.toml'))
        require(set(config) == {'strict_mode'}, 'Default config unexpectedly introduces policy/list settings')
    case('PKG-ALLOW-001', no_allowlist)
    for identifier in ['PKG-UPGRADE-001', 'PKG-UPGRADE-002']:
        results.append({'id': identifier, 'status': 'skip', 'reason': 'Initial release: no previous accepted package; upgrade implementation deferred.'})
    return results


def inspect_extension(path, browser):
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        require(len(names) == len(set(names)), 'Duplicate extension archive members')
        require(all(not PurePosixPath(p).is_absolute() and '..' not in PurePosixPath(p).parts for p in names), 'Unsafe extension path')
        manifest = json.loads(archive.read('manifest.json'))
        source = json.loads((ROOT / f'browser-extension-{browser}/manifest.json').read_text())
        package = json.loads((ROOT / f'browser-extension-{browser}/package.json').read_text())
        require(manifest == source and manifest['version'] == package['version'], 'Extension manifest/version differs from tested source')
        required = ['blocked.html', 'dist/background.js', 'dist/blocked.js', *manifest.get('icons', {}).values()]
        background = manifest['background']
        required += background.get('scripts', []) + ([background['service_worker']] if 'service_worker' in background else [])
        for name in required:
            require(bool(archive.read(name)), f'Missing/empty extension asset: {name}')
        for name in ['dist/background.js', 'dist/blocked.js']:
            require(archive.read(name) == (ROOT / f'browser-extension-{browser}' / name).read_bytes(), f'Stale built script: {name}')
        require('nativeMessaging' in manifest['permissions'], 'Missing Native Messaging permission')
        return manifest['version']
