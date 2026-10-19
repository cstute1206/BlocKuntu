#!/usr/bin/env python3
"""Regression tests using real archive formats and deliberately broken contents."""
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from package_inspect import CONTRACT, ROOT, archive_entries, inspect_extension, inspect_package, read_package


class PackageTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        # Exercise the production Debian assembly with obvious fixture ELF headers.
        source = cls.root / 'source'
        source.mkdir()
        for directory in ['scripts', 'packaging', 'focus-gui/src-tauri/icons']:
            shutil.copytree(ROOT / directory, source / directory)
        for name in ['LICENSE', 'README.md']:
            (source / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, source / name)
        header = bytearray(64)
        header[:6] = b'\x7fELF\x02\x01'
        header[18:20] = b'\x3e\x00'
        for directory, name in [('focusd', 'blockuntud'), ('native-host', 'blockuntu-native'), ('focus-gui/src-tauri', 'blockuntu-gui')]:
            binary = source / directory / 'target/release' / name
            binary.parent.mkdir(parents=True, exist_ok=True)
            binary.write_bytes(header)
            binary.chmod(0o755)
        (source / 'target').mkdir()
        (source / 'target/.blockuntu-package-version').write_text('0.2.0-1\n')
        subprocess.run(['bash', str(source / 'scripts/package-deb.sh'), '--no-build'], cwd=source, check=True, stdout=subprocess.DEVNULL)
        cls.deb = source / 'target/debian/blockuntu_0.2.0-1_amd64.deb'
        cls.entries, cls.meta, cls.hooks = read_package(cls.deb, 'deb')

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def assert_case_fails(self, case, entries=None, meta=None, hooks=None):
        results = inspect_package(entries or self.entries, meta or self.meta, hooks or self.hooks, 'deb')
        self.assertEqual(next(row['status'] for row in results if row['id'] == case), 'fail', results)

    def test_production_debian_assembly(self):
        results = inspect_package(self.entries, self.meta, self.hooks, 'deb')
        self.assertFalse([row for row in results if row['status'] == 'fail'], results)
        self.assertEqual([row['id'] for row in results if row['status'] == 'skip'], ['PKG-UPGRADE-001', 'PKG-UPGRADE-002'])

    def test_missing_binary(self):
        entries = copy.deepcopy(self.entries)
        del entries['usr/bin/blockuntu-native']
        self.assert_case_fails('PKG-CONTENT-001', entries=entries)

    def test_rejects_shared_firefox_policy_deletion_in_native_hook(self):
        hooks = copy.deepcopy(self.hooks)
        hooks['prerm'] += '\nrm -f /etc/firefox/policies/policies.json\n'
        self.assert_case_fails('PKG-CONTENT-005', hooks=hooks)

    def test_wrong_elf_architecture(self):
        entries = copy.deepcopy(self.entries)
        entries['usr/bin/blockuntud']['data'] = b'#!/bin/sh\n'
        self.assert_case_fails('PKG-CONTENT-001', entries=entries)

    def test_wrong_permissions_and_owner(self):
        for key, value in [('mode', 0o777), ('uid', 1000), ('mode', 0o4755)]:
            with self.subTest(key=key, value=value):
                entries = copy.deepcopy(self.entries)
                entries['usr/bin/blockuntud'][key] = value
                self.assert_case_fails('PKG-CONTENT-009', entries=entries)

    def test_bad_manifest(self):
        entries = copy.deepcopy(self.entries)
        path = 'etc/chromium/native-messaging-hosts/blockuntu_native.json'
        entries[path]['data'] = entries[path]['data'].replace(b'/usr/bin/', b'/usr/local/bin/')
        self.assert_case_fails('PKG-CONTENT-004', entries=entries)
        self.assert_case_fails('PKG-CONTENT-010', entries=entries)

    def test_rpm_requires_native_firefox_manifest_location(self):
        entries = copy.deepcopy(self.entries)
        for browser in ('mozilla', 'librewolf', 'waterfox'):
            path = f'usr/lib/{browser}/native-messaging-hosts/blockuntu_native.json'
            entries[path.replace('usr/lib/', 'usr/lib64/', 1)] = copy.deepcopy(entries[path])
        def manifest_status():
            rows = inspect_package(entries, self.meta, self.hooks, 'rpm')
            return next(row['status'] for row in rows if row['id'] == 'PKG-CONTENT-004')
        self.assertEqual(manifest_status(), 'pass')
        del entries['usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json']
        self.assertEqual(manifest_status(), 'fail')

    def test_bad_defaults_and_empty_allowlist(self):
        entries = copy.deepcopy(self.entries)
        path = 'etc/blockuntu/config.toml'
        entries[path]['data'] = entries[path]['data'].replace(b'= true', b'= false')
        self.assert_case_fails('PKG-CONTENT-003', entries=entries)
        entries[path]['data'] += b'\n[application_allowlist]\nentries = []\n'
        self.assert_case_fails('PKG-ALLOW-001', entries=entries)

    def test_missing_helper_and_watchdog(self):
        entries = copy.deepcopy(self.entries)
        del entries['usr/lib/blockuntu/setup-confined-chromium-native-host.sh']
        del entries['lib/systemd/system/blockuntu-watchdog.service']
        self.assert_case_fails('PKG-CONTENT-006', entries=entries)
        self.assert_case_fails('PKG-HARD-002', entries=entries)

    def test_hosts_watcher_boot_order_cycle(self):
        entries = copy.deepcopy(self.entries)
        entries['lib/systemd/system/blockuntu-hosts.path']['data'] = entries[
            'lib/systemd/system/blockuntu-hosts.path']['data'].replace(b'[Unit]', b'[Unit]\nAfter=blockuntu.service')
        self.assert_case_fails('PKG-HARD-002', entries=entries)

    def test_daemon_restart_cannot_unlink_socket_runtime_directory(self):
        entries = copy.deepcopy(self.entries)
        path = 'lib/systemd/system/blockuntu.service'
        entries[path]['data'] = entries[path]['data'].replace(b'RuntimeDirectoryPreserve=restart\n', b'')
        self.assert_case_fails('PKG-HARD-002', entries=entries)

    def test_user_native_cleanup_is_shipped_and_called(self):
        entries = copy.deepcopy(self.entries)
        del entries['usr/lib/blockuntu/remove-user-native-hosts.sh']
        self.assert_case_fails('PKG-CONTENT-006', entries=entries)
        hooks = {name: body.replace('/usr/lib/blockuntu/remove-user-native-hosts.sh', ':') for name, body in self.hooks.items()}
        self.assert_case_fails('PKG-CONTENT-008', hooks=hooks)

    def cleanup_fixture(self, home):
        helper = home.parent / 'packaged-cleanup.sh'
        helper.write_bytes(self.entries['usr/lib/blockuntu/remove-user-native-hosts.sh']['data'])
        helper.chmod(0o755)
        command = ['/bin/sh', str(helper), '--user', str(home)]
        if os.geteuid() == 0:
            # Package-inspector tests may also run in a root container. Cleanup
            # must still execute as an ordinary account, with an owned fixture.
            for path in [home.parent, *home.parent.rglob('*')]:
                os.chown(path, 65534, 65534)
            command = ['runuser', '-u', 'nobody', '--', *command]
        subprocess.run(command, check=True, capture_output=True)

    def test_confirmed_six_user_native_leftovers_removed_without_other_files(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / 'home with spaces'
            managed = [
                '.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/blockuntu_native.json',
                'snap/chromium/common/chromium/NativeMessagingHosts/blockuntu_native.json',
                'snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json',
                'snap/chromium/common/.local/share/blockuntu/blockuntu-native',
                'snap/chromium/common/.local/share/blockuntu/blockuntu-native-launcher',
                'snap/firefox/common/.local/share/blockuntu/blockuntu-native',
            ]
            for name in managed:
                path = home / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text('managed fixture')
            sentinels = [home / '.mozilla/prefs.js', home / 'snap/firefox/common/.mozilla/native-messaging-hosts/other.json',
                         home / 'snap/chromium/common/.local/share/blockuntu/unrelated.txt']
            for path in sentinels:
                path.parent.mkdir(parents=True, exist_ok=True); path.write_text('keep me')
            self.cleanup_fixture(home)
            self.assertTrue(all(not (home / name).exists() for name in managed))
            self.assertTrue(all(path.read_text() == 'keep me' for path in sentinels))
            self.cleanup_fixture(home)  # Final-removal retries must be harmless.

    def test_flatpak_and_snap_revision_native_cleanup_keeps_profiles(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / 'home'
            managed = [home / '.var/app/org.mozilla.firefox/data/blockuntu/blockuntu-native',
                       home / '.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/blockuntu_native.json']
            for browser, profile in [('brave', 'BraveSoftware/Brave-Browser'), ('opera', 'google-chrome'), ('vivaldi', 'vivaldi')]:
                for revision in ('123', '456'):
                    path = home / 'snap' / browser / revision / '.config' / profile / 'NativeMessagingHosts/blockuntu_native.json'
                    path.parent.mkdir(parents=True); managed.append(path)
                (home / 'snap' / browser / 'current').symlink_to('456', target_is_directory=True)
            for path in managed:
                path.parent.mkdir(parents=True, exist_ok=True); path.write_text('managed fixture')
            sentinel = home / 'snap/brave/456/.config/BraveSoftware/Brave-Browser/Preferences'
            sentinel.write_text('keep profile')
            self.cleanup_fixture(home)
            self.assertTrue(all(not path.exists() for path in managed))
            self.assertEqual(sentinel.read_text(), 'keep profile')
            self.assertTrue((home / 'snap/brave/current').is_symlink())

    def test_user_cleanup_refuses_root_privileges_before_touching_home(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory) / 'home'
            path = home / '.mozilla/native-messaging-hosts/blockuntu_native.json'
            path.parent.mkdir(parents=True); path.write_text('keep')
            fake_bin = Path(directory) / 'bin'; fake_bin.mkdir()
            fake_id = fake_bin / 'id'; fake_id.write_text('#!/bin/sh\necho 0\n'); fake_id.chmod(0o755)
            result = subprocess.run(['/bin/sh', str(ROOT / 'scripts/remove-user-native-hosts.sh'), '--user', str(home)],
                                    env={**os.environ, 'PATH': str(fake_bin) + ':' + os.environ['PATH']}, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(path.read_text(), 'keep')

    def test_cleanup_dispatch_drops_privileges_and_excludes_system_accounts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); home = root / 'home with spaces'; home.mkdir()
            manifest = home / '.mozilla/native-messaging-hosts/blockuntu_native.json'
            manifest.parent.mkdir(parents=True); manifest.write_text('must not delete as root')
            passwd = root / 'passwd'
            passwd.write_text('\n'.join(f'{name}:x:{uid}:1000::{home}:/bin/sh' for name,uid in
                                       [('root',0),('service',1),('nobody',65534),('desktop',2147483000)])+'\n')
            log = root / 'runuser.json'; fake_bin = root / 'bin'; fake_bin.mkdir()
            tools = {'id':'#!/bin/sh\necho 0\n',
                     'getent':'#!/bin/sh\nexec /bin/cat "$PASSWD_FIXTURE"\n',
                     'runuser':f'#!{sys.executable}\nimport json,os,sys\nopen(os.environ["RUNUSER_LOG"],"w").write(json.dumps(sys.argv[1:]))\n'}
            for name,body in tools.items():
                path = fake_bin / name; path.write_text(body); path.chmod(0o755)
            subprocess.run(['/bin/sh',str(ROOT / 'scripts/remove-user-native-hosts.sh')], check=True, capture_output=True,
                env={**os.environ,'PATH':str(fake_bin)+':'+os.environ['PATH'],'PASSWD_FIXTURE':str(passwd),'RUNUSER_LOG':str(log)})
            self.assertEqual(json.loads(log.read_text()), ['-u','desktop','--','/bin/sh',str(ROOT / 'scripts/remove-user-native-hosts.sh'),'--user',str(home)])
            self.assertEqual(manifest.read_text(), 'must not delete as root')

    def test_bad_metadata(self):
        for key, value, case in [('version', '0.1.0-26', 'PKG-META-001'), ('architecture', 'arm64', 'PKG-META-002'), ('dependencies', [], 'PKG-META-002')]:
            meta = copy.deepcopy(self.meta)
            meta[key] = value
            self.assert_case_fails(case, meta=meta)

    def test_missing_authorization(self):
        hooks = {key: value.replace('BLOCKUNTU_PACKAGE_REMOVAL_LEASE', 'WRONG') for key, value in self.hooks.items()}
        self.assert_case_fails('PKG-HARD-001', hooks=hooks)

    def test_unsafe_archive_paths_links_duplicates(self):
        for names, link in [(['../escape'], False), (['/absolute'], False), (['duplicate', 'duplicate'], False), (['link'], True)]:
            with self.subTest(names=names, link=link):
                stream = io.BytesIO()
                with tarfile.open(fileobj=stream, mode='w') as archive:
                    for name in names:
                        member = tarfile.TarInfo(name)
                        if link:
                            member.type = tarfile.SYMTYPE
                            member.linkname = '/etc/hosts'
                        archive.addfile(member)
                stream.seek(0)
                with self.assertRaises(ValueError):
                    archive_entries(stream)

    def test_rpm_build_id_links(self):
        for destination, valid in [('../../../bin/blockuntud', True), ('../../../../etc/shadow', False)]:
            with self.subTest(destination=destination):
                stream = io.BytesIO()
                with tarfile.open(fileobj=stream, mode='w') as archive:
                    binary = tarfile.TarInfo('usr/bin/blockuntud')
                    binary.size = 3
                    archive.addfile(binary, io.BytesIO(b'ELF'))
                    link = tarfile.TarInfo('usr/lib/.build-id/aa/bb')
                    link.type = tarfile.SYMTYPE
                    link.linkname = destination
                    archive.addfile(link)
                stream.seek(0)
                if valid:
                    self.assertEqual(archive_entries(stream)['usr/lib/.build-id/aa/bb']['link'], 'usr/bin/blockuntud')
                else:
                    with self.assertRaises(ValueError):
                        archive_entries(stream)

    def test_arch_reader(self):
        package = self.root / 'fixture.pkg.tar.gz'
        metadata = 'pkgname = blockuntu\npkgver = 0.2.0-1\narch = x86_64\nlicense = MIT\npkgdesc = Fixture\ndepend = systemd>=1\n'
        with tarfile.open(package, 'w:gz') as archive:
            for name, text in [('.PKGINFO', metadata), ('.INSTALL', 'post_install() { :; }\n')]:
                member = tarfile.TarInfo(name)
                member.size = len(text)
                archive.addfile(member, io.BytesIO(text.encode()))
        _, meta, hooks = read_package(package, 'arch')
        self.assertEqual(meta['version'], '0.2.0-1')
        self.assertEqual(meta['dependencies'], ['systemd'])
        self.assertIn('post_install', hooks['install'])

    def arch_assembly(self, mask):
        work = Path(tempfile.mkdtemp(prefix='arch-assembly-' + mask + '-', dir=self.root))
        source = work / 'source'
        source.mkdir()
        (source / 'blockuntu-0.2.0').symlink_to(self.root / 'source', target_is_directory=True)
        package_root = work / 'package'
        package_root.mkdir()
        subprocess.run(['bash', '-c', 'umask "$4"; source "$1"; srcdir="$2"; pkgdir="$3"; package',
                        'arch-fixture', str(ROOT / 'packaging/arch/PKGBUILD'), str(source), str(package_root), mask], check=True)
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as archive:
            def root_owner(member):
                member.uid = member.gid = 0
                member.uname = member.gname = 'root'
                return member
            for path in sorted(package_root.rglob('*')):
                archive.add(path, arcname=str(path.relative_to(package_root)), recursive=False, filter=root_owner)
        stream.seek(0)
        return archive_entries(stream)

    def test_arch_assembly_permissions_do_not_depend_on_caller_umask(self):
        for mask in ('0002', '0022'):
            with self.subTest(umask=mask):
                entries = self.arch_assembly(mask)
                self.assertEqual(entries['etc/chromium']['mode'], 0o755)
                self.assertFalse([name for name, entry in entries.items()
                                  if not entry.get('link') and entry['mode'] & 0o7022])

    def test_production_arch_assembly(self):
        entries = self.arch_assembly('0022')
        meta = {'name': 'blockuntu', 'version': '0.2.0-1', 'architecture': 'x86_64',
                'license': 'MIT', 'description': 'Fixture', 'dependencies': CONTRACT['targets']['arch']['dependencies']}
        hooks = {'install': (ROOT / 'packaging/arch/blockuntu.install').read_text()}
        results = inspect_package(entries, meta, hooks, 'arch')
        self.assertFalse([row for row in results if row['status'] == 'fail'], results)
        guard_path = 'usr/share/libalpm/hooks/00-blockuntu-removal.hook'
        for old, new in [(b'AbortOnFail', b''), (b'PreTransaction', b'PostTransaction'),
                         (b'Operation = Remove', b'Operation = Upgrade')]:
            broken = copy.deepcopy(entries)
            broken[guard_path]['data'] = broken[guard_path]['data'].replace(old, new)
            inspected = inspect_package(broken, meta, hooks, 'arch')
            self.assertEqual(next(row['status'] for row in inspected if row['id'] == 'PKG-HARD-001'), 'fail')
        broken = copy.deepcopy(entries)
        del broken['usr/lib/blockuntu/check-package-removal-lease.sh']
        inspected = inspect_package(broken, meta, hooks, 'arch')
        self.assertEqual(next(row['status'] for row in inspected if row['id'] == 'PKG-HARD-001'), 'fail')

        # Exercise the RPM guard contract with the production action file.
        rpm_entries = copy.deepcopy(entries)
        rpm_entries['usr/libexec/blockuntu/check-package-removal-lease.sh'] = copy.deepcopy(
            entries['usr/lib/blockuntu/check-package-removal-lease.sh'])
        action_path = 'etc/dnf/libdnf5-plugins/actions.d/blockuntu-removal.actions'
        rpm_entries[action_path] = {**entries[guard_path], 'data': (ROOT / 'packaging/rpm/blockuntu-removal.actions').read_bytes()}
        def guard_status():
            rows = inspect_package(rpm_entries, meta, hooks, 'rpm')
            return next(row['status'] for row in rows if row['id'] == 'PKG-HARD-001')
        self.assertEqual(guard_status(), 'pass')
        rpm_entries[action_path]['data'] = rpm_entries[action_path]['data'].replace(b'raise_error=1', b'raise_error=0')
        self.assertEqual(guard_status(), 'fail')

    def test_rpm_reader(self):
        top = self.root / 'rpm'
        top.mkdir(exist_ok=True)
        spec = top / 'fixture.spec'
        spec.write_text('''Name: blockuntu
Version: 0.2.0
Release: 1
Summary: Fixture
License: MIT
Requires: systemd
%description
Fixture only.
%install
mkdir -p %{buildroot}/usr/share/blockuntu
printf fixture > %{buildroot}/usr/share/blockuntu/fixture
%post
: postinstall
%preun
: removal
%postun
: cleanup
%files
/usr/share/blockuntu/fixture
''')
        subprocess.run(['rpmbuild', '-bb', '--define', '_rpmformat 4', '--define', f'_topdir {top}', '--define', f'_tmppath {top}', str(spec)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        package = next((top / 'RPMS').rglob('*.rpm'))
        entries, meta, hooks = read_package(package, 'rpm')
        self.assertEqual(meta['version'], '0.2.0-1')
        self.assertEqual(meta['architecture'], 'x86_64')
        self.assertIn('systemd', meta['dependencies'])
        self.assertIn('postinstall', hooks['postinst'])
        self.assertEqual(entries['usr/share/blockuntu/fixture']['data'], b'fixture')

    def test_builder_versions_and_locked_tauri(self):
        version = CONTRACT['version']
        for name in ['package-deb.sh', 'package-rpm.sh', 'package-arch.sh', 'package-arch-docker.sh']:
            data = (ROOT / 'scripts' / name).read_text()
            expected = version + '-1' if name == 'package-deb.sh' else version
            self.assertIn(f'VERSION="{expected}"', data)
        for path in ['packaging/rpm/blockuntu.spec', 'packaging/arch/PKGBUILD']:
            self.assertIn('build --no-bundle -- --locked', (ROOT / path).read_text())

    def test_failed_inspection_writes_report(self):
        spec = importlib.util.spec_from_file_location('package_ci_runner', ROOT / 'Testing/scripts/package-ci.py')
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        artifact = self.root / 'corrupt.deb'
        artifact.write_text('not a package')
        directory = self.root / 'failure-report'
        with patch.object(sys, 'argv', ['package-ci.py', 'deb', '--artifact', str(artifact), '--output-dir', str(directory)]):
            self.assertEqual(runner.main(), 1)
        report = json.loads((directory / 'result.json').read_text())
        self.assertFalse(report['passed'])
        self.assertTrue(any(row['status'] == 'blocked' for row in report['cases']))
        self.assertTrue((directory / 'report.md').exists())

    def test_dirty_checkout_cannot_claim_release_build(self):
        spec = importlib.util.spec_from_file_location('package_ci_clean_guard', ROOT / 'Testing/scripts/package-ci.py')
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        directory = self.root / 'dirty-checkout-report'
        def capture(command):
            value = ' M changed-source.rs' if command[:2] == ['git', 'status'] else 'fixture'
            return {'command': command, 'exit_code': 0, 'stdout': value, 'stderr': ''}
        with patch.object(sys, 'argv', ['package-ci.py', 'deb', '--output-dir', str(directory)]), patch.object(runner, 'capture', capture), patch.object(runner, 'run_command') as build:
            self.assertEqual(runner.main(), 1)
            build.assert_not_called()
        report = json.loads((directory / 'result.json').read_text())
        self.assertFalse(report['build_provenance_verified'])
        self.assertIn('clean committed checkout', report['error'])

    def test_extension_missing_asset(self):
        import zipfile
        path = self.root / 'bad.zip'
        with zipfile.ZipFile(path, 'w') as archive:
            archive.write(ROOT / 'browser-extension-chrome/manifest.json', 'manifest.json')
        with self.assertRaises(KeyError):
            inspect_extension(path, 'chrome')


if __name__ == '__main__':
    unittest.main()
