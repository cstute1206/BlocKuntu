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

    def test_production_arch_assembly(self):
        work = self.root / 'arch-assembly'
        work.mkdir()
        source = work / 'source'
        source.mkdir()
        (source / 'blockuntu-0.2.0').symlink_to(self.root / 'source', target_is_directory=True)
        package_root = work / 'package'
        package_root.mkdir()
        subprocess.run(['bash', '-c', 'source "$1"; srcdir="$2"; pkgdir="$3"; package',
                        'arch-fixture', str(ROOT / 'packaging/arch/PKGBUILD'), str(source), str(package_root)], check=True)
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as archive:
            def root_owner(member):
                member.uid = member.gid = 0
                member.uname = member.gname = 'root'
                return member
            for path in sorted(package_root.rglob('*')):
                archive.add(path, arcname=str(path.relative_to(package_root)), recursive=False, filter=root_owner)
        stream.seek(0)
        entries = archive_entries(stream)
        meta = {'name': 'blockuntu', 'version': '0.2.0-1', 'architecture': 'x86_64',
                'license': 'MIT', 'description': 'Fixture', 'dependencies': CONTRACT['targets']['arch']['dependencies']}
        hooks = {'install': (ROOT / 'packaging/arch/blockuntu.install').read_text()}
        results = inspect_package(entries, meta, hooks, 'arch')
        self.assertFalse([row for row in results if row['status'] == 'fail'], results)

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
        subprocess.run(['rpmbuild', '-bb', '--define', f'_topdir {top}', '--define', f'_tmppath {top}', str(spec)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
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
