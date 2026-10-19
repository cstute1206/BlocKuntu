"""Native adapter regressions: evidence identity and authorization boundaries."""
import datetime as dt
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch, Mock
import sys
import json
import test_layer4_hosts as hosts_tests
hosts_auditor = hosts_tests.auditor
from layer4_hosts_contract import CASES as HOST_CASES

from layer4_platform import (browser_paths, browsers_for, detected_guest,
                             direct_removal, removal_argv)
from layer4_uninstall_contract import native_removal_witness, verify_guest
from layer4_browser_contract import fault_paths

spec = importlib.util.spec_from_file_location('port_ops', Path(__file__).parents[1] / 'fixtures/layer4/uninstall_ops.py')
ops = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ops)


class NativeAdapters(unittest.TestCase):
    @staticmethod
    def named_page_fixture():
        guest = type(sys)('guest')
        guest.WORK = Path('/unused')
        for name in ('configure_desktop', 'host_action', 'rpc', 'run', 'wait'):
            setattr(guest, name, Mock())
        spec = importlib.util.spec_from_file_location('named_page_fixture',
            Path(__file__).parents[1] / 'fixtures/layer4/uninstall.py')
        fixture = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {'guest': guest}):
            spec.loader.exec_module(fixture)
        return fixture

    def test_onboarding_waits_for_real_named_policy_page_after_transient_fail_closed(self):
        fixture = self.named_page_fixture()
        url = 'http://web.blockuntu.test:18080/firefox/exact/blocked'
        rule = {'name': 'P41 exact firefox', 'patterns': [{'value': url}]}
        backend = 'Blocked by BlocKuntu\nDaemon heartbeat missing\n' + url
        named = 'Blocked by BlocKuntu\nP41 exact firefox\n' + url
        fixture.visible_document_text = Mock(side_effect=[backend, named])
        def bounded(ready, seconds):
            self.assertEqual(seconds, 45)
            self.assertIsNone(ready())
            return ready()
        fixture.wait = bounded
        self.assertEqual(fixture.wait_named_block_page('Firefox', rule), named)

    def test_onboarding_does_not_accept_generic_block_title_wrong_rule_or_wrong_url(self):
        fixture = self.named_page_fixture()
        url = 'http://web.blockuntu.test:18080/firefox/exact/blocked'
        rule = {'name': 'P41 exact firefox', 'patterns': [{'value': url}]}
        for text in ('Blocked by BlocKuntu\nDaemon heartbeat missing\n' + url,
                     'Blocked by BlocKuntu\nOther rule\n' + url,
                     'Blocked by BlocKuntu\nP41 exact firefox\nhttp://other.test/'):
            fixture.visible_document_text = Mock(return_value=text)
            def timeout(ready, seconds):
                self.assertIsNone(ready())
                raise TimeoutError('deadline ended')
            fixture.wait = timeout
            with self.assertRaisesRegex(TimeoutError, 'No named block page'):
                fixture.wait_named_block_page('Firefox', rule)

    def test_hosts_mapping_wait_observes_stale_nss_then_actual_new_mapping(self):
        import layer4_platform as platform
        stale = {'native': {'returncode': 0, 'addresses': ['127.0.0.2']},
                 'files': {'returncode': 0, 'addresses': ['127.0.0.43']},
                 'nss': {'errno': None, 'addresses': ['127.0.0.2']}}
        matched = json.loads(json.dumps(stale))
        matched['native']['addresses'] = matched['nss']['addresses'] = ['127.0.0.43']
        def waiter(ready, seconds):
            self.assertEqual(seconds, 30)
            self.assertIsNone(ready())
            return ready()
        with patch.object(platform, 'resolution_witness', side_effect=[stale, matched]):
            proof = platform.wait_hosts_mapping('sentinel.test', '127.0.0.43', Mock(), waiter)
        self.assertTrue(proof['settled'])
        self.assertEqual(len(proof['samples']), 2)
        self.assertEqual(proof['samples'][0]['nss']['addresses'], ['127.0.0.2'])
        self.assertEqual(proof['samples'][-1]['nss']['addresses'], ['127.0.0.43'])

    def test_hosts_mapping_timeout_retains_actual_native_answers(self):
        import layer4_platform as platform
        stale = {'native': {'returncode': 0, 'addresses': ['127.0.0.2']},
                 'files': {'returncode': 0, 'addresses': ['127.0.0.43']},
                 'nss': {'errno': None, 'addresses': ['127.0.0.2']}}
        def timeout(ready, seconds):
            self.assertIsNone(ready())
            raise TimeoutError('bounded window ended')
        with patch.object(platform, 'resolution_witness', return_value=stale):
            with self.assertRaisesRegex(TimeoutError, '127.0.0.2'):
                platform.wait_hosts_mapping('sentinel.test', '127.0.0.43', Mock(), timeout)

    def test_hosts_mapping_rejects_partial_conflicting_or_failed_interfaces(self):
        from layer4_platform import hosts_mapping_resolution
        valid = {'native': {'returncode': 0, 'addresses': ['127.0.0.43']},
                 'files': {'returncode': 0, 'addresses': ['127.0.0.43']},
                 'nss': {'errno': None, 'addresses': ['127.0.0.43']}}
        self.assertTrue(hosts_mapping_resolution(valid, '127.0.0.43'))
        for key, field, value in [('native', 'addresses', ['127.0.0.2']),
                ('files', 'addresses', []), ('native', 'returncode', 2),
                ('native', 'returncode', False), ('nss', 'errno', -3),
                ('nss', 'addresses', [])]:
            invalid = json.loads(json.dumps(valid))
            invalid[key][field] = value
            self.assertFalse(hosts_mapping_resolution(invalid, '127.0.0.43'), invalid)
        self.assertFalse(hosts_mapping_resolution({}, '127.0.0.43'))

    def test_hosts_mapping_waiter_cannot_turn_an_unmatched_callback_into_a_pass(self):
        import layer4_platform as platform
        with patch.object(platform, 'resolution_witness', return_value={}):
            with self.assertRaisesRegex(TimeoutError, 'did not settle'):
                platform.wait_hosts_mapping('sentinel.test', '127.0.0.43', Mock(),
                                            lambda ready, seconds: ready())

    def test_blocked_nss_requires_files_mapping_and_normal_nss_denial(self):
        import socket
        from layer4_platform import blocking_resolution
        def witness(code, addresses, errno=None, nss_addresses=None, file_code=0, file_addresses=('0.0.0.0',)):
            return {'native': {'returncode': code, 'addresses': addresses},
                    'files': {'returncode': file_code, 'addresses': list(file_addresses)},
                    'nss': {'errno': errno, 'addresses': addresses if nss_addresses is None else nss_addresses}}
        self.assertTrue(blocking_resolution(witness(0, ['0.0.0.0'])))
        self.assertTrue(blocking_resolution(witness(2, [], socket.EAI_NONAME)))
        self.assertTrue(blocking_resolution(witness(15, [], socket.EAI_NODATA)))
        for invalid in (witness(0, []), witness(1, []), witness(15, [], socket.EAI_AGAIN),
                        witness(15, [], socket.EAI_SYSTEM), witness(15, [], socket.EAI_NODATA, file_code=2, file_addresses=()),
                        witness(15, [], socket.EAI_NODATA, file_addresses=('127.0.0.2',)),
                        witness(0, ['127.0.0.2']), witness(0, ['0.0.0.0', '127.0.0.2'])):
            self.assertFalse(blocking_resolution(invalid), invalid)

    def test_resolution_witness_keeps_normal_files_and_bounded_api_queries_separate(self):
        from layer4_platform import resolution_witness, NSS_QUERY
        command = Mock(side_effect=[Mock(returncode=15, stdout='', stderr=''),
            Mock(returncode=0, stdout='0.0.0.0 STREAM blocked.test\n0.0.0.0 DGRAM\n', stderr=''),
            Mock(returncode=0, stdout=json.dumps({'addresses': [], 'errno': -5, 'error': 'No address'}))])
        witness = resolution_witness('blocked.test', command)
        self.assertEqual(witness['native']['returncode'], 15)
        self.assertEqual(witness['files']['addresses'], ['0.0.0.0'])
        self.assertEqual(witness['nss']['errno'], -5)
        self.assertEqual(command.call_args_list[0].args, ('getent', 'ahostsv4', 'blocked.test'))
        self.assertEqual(command.call_args_list[1].args, ('getent', '-s', 'files', 'ahostsv4', 'blocked.test'))
        self.assertEqual(command.call_args_list[2].args, ('/usr/bin/python3', '-c', NSS_QUERY, 'blocked.test'))
        self.assertEqual(command.call_args_list[2].kwargs['timeout'], 5)

    def test_native_nss_script_reports_actual_literal_address_without_network(self):
        from layer4_platform import NSS_QUERY
        answer = subprocess.run([sys.executable, '-c', NSS_QUERY, '127.0.0.1'],
            check=True, capture_output=True, text=True, timeout=5)
        self.assertEqual(json.loads(answer.stdout), {'addresses': ['127.0.0.1'], 'errno': None, 'error': None})

    def test_crashed_nss_probe_cannot_become_a_denied_lookup(self):
        from layer4_platform import resolution_witness
        command = Mock(side_effect=[Mock(returncode=15, stdout='', stderr=''),
            Mock(returncode=0, stdout='0.0.0.0 STREAM blocked.test', stderr=''),
            Mock(returncode=13, stdout='')])
        with self.assertRaisesRegex(RuntimeError, 'Bounded native NSS probe failed'):
            resolution_witness('blocked.test', command)

    def test_rolling_fixture_refresh_cannot_upgrade_an_existing_blockuntu_install(self):
        for script in ('uninstall-acceptance.py', 'hosts-acceptance.py', 'browser-acceptance.py'):
            spec = importlib.util.spec_from_file_location('refresh_guard_' + script,
                Path(__file__).parent / script)
            runner = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(runner)
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                package = root / 'candidate.pkg.tar.zst'
                package.write_bytes(b'guarded-test-candidate')
                (root / 'evidence').mkdir()
                record = {'evidence': str(root / 'evidence'), 'template': 'cachyos',
                    'uuid': 'test-clone', 'domain': 'owned-test-clone', 'run_id': 'refresh-guard'}
                with patch.object(runner.vm, 'prepare', return_value=record), \
                        patch.object(runner.vm, 'verify'), patch.object(runner.suite, 'boot'), \
                        patch.object(runner, 'checked', return_value=Mock(returncode=0)), \
                        patch.object(runner.suite, 'prepare_cachyos') as refresh, \
                        patch.object(runner, 'upload') as upload, patch.object(runner, 'finalize'):
                    result, _ = runner.run_scenario('refresh-guard', 'remove' if script != 'browser-acceptance.py' else 'policy',
                        package, {'commit': 'a' * 40}, 'cachyos', True)
                self.assertEqual(result['status'], 'fail')
                self.assertIn('BlocKuntu absent', result['error'])
                refresh.assert_not_called()
                upload.assert_not_called()

    def test_native_refusal_includes_surviving_gui_uninstall_path(self):
        from layer4_uninstall_contract import cases_for
        for guest in ('fedora', 'cachyos'):
            baseline, refusal = cases_for(guest)['remove']
            baseline_proof = dict.fromkeys(('installed', 'clock_healthy', 'hosts_blocked',
                'process_blocked', 'near_miss_alive', 'collector_alive'), True)
            refusal_proof = dict.fromkeys(('command_rejected', 'package_installed',
                'hosts_blocked', 'process_blocked', 'near_miss_alive', 'no_lease'), True)
            result = {'scenario': 'remove', 'guest': guest, 'status': 'pass', 'ordinary_uid': 1000,
                'cases': [{'id': baseline, 'variant': 'remove', 'status': 'pass', 'evidence': baseline_proof},
                          {'id': refusal, 'variant': 'remove', 'status': 'pass', 'evidence': refusal_proof}]}
            with self.assertRaisesRegex(ValueError, 'Incomplete evidence'):
                verify_guest(result, 'remove', guest)
            refusal_proof['gui_restart_healthy'] = True
            with self.assertRaisesRegex(ValueError, 'Incomplete evidence'):
                verify_guest(result, 'remove', guest)
            refusal_proof['all_packages_preserved'] = True
            self.assertEqual(verify_guest(result, 'remove', guest), 2)

    def test_accessibility_walk_ignores_transient_null_children(self):
        stubs = {'pyatspi': Mock()}
        with patch.dict(sys.modules, stubs):
            spec = importlib.util.spec_from_file_location('null_child_probe',
                Path(__file__).parents[1] / 'fixtures/layer3/accessibility.py')
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            leaf = Mock()
            leaf.__iter__ = Mock(return_value=iter(()))
            parent = Mock()
            parent.__iter__ = Mock(return_value=iter((None, leaf)))
            self.assertEqual(list(module.walk(parent)), [parent, leaf])

    def test_refusal_witness_uses_the_actual_native_package_message(self):
        from layer4_platform import refusal_message
        self.assertEqual(refusal_message('cachyos'), 'BlocKuntu refuses direct Pacman removal')
        self.assertEqual(refusal_message('fedora'), 'BlocKuntu refuses direct package-manager removal')
        with self.assertRaises(KeyError):
            refusal_message('untrusted')

    def test_os_identity_does_not_silently_select_another_adapter(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'os-release'
            for identity, guest in [('ubuntu', 'ubuntu'), ('fedora', 'fedora'), ('cachyos', 'cachyos'), ('arch', 'cachyos')]:
                path.write_text('ID="' + identity + '"\n')
                self.assertEqual(detected_guest(path), guest)
            path.write_text('ID=unknown\nID_LIKE=arch\n')
            with self.assertRaises(ValueError):
                detected_guest(path)

    def test_cachyos_uses_actual_chromium_component_and_fixed_paths(self):
        self.assertEqual(browsers_for('cachyos'), ('firefox', 'chromium'))
        policies, manifests = browser_paths('cachyos')
        self.assertIn('/etc/chromium/', policies[1])
        self.assertIn('/etc/chromium/', manifests[1])
        with self.assertRaises(ValueError):
            browser_paths('untrusted')
        with self.assertRaises(ValueError):
            direct_removal('fedora', 'remove; reboot')

    def test_fedora_faults_cover_rpm_and_native_release_registrations(self):
        self.assertEqual(fault_paths('fedora')[1][0], '/usr/lib64/mozilla/native-messaging-hosts/blockuntu_native.json')
        self.assertEqual(set(fault_paths('fedora')[1]), {
            '/usr/lib64/mozilla/native-messaging-hosts/blockuntu_native.json',
            '/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json',
            '/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json'})
        self.assertEqual(fault_paths('ubuntu')[1][0], '/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json')
        self.assertEqual(fault_paths('cachyos')[1][1], '/etc/chromium/native-messaging-hosts/blockuntu_native.json')

    def test_fedora_manifest_fault_disables_both_registrations_and_restores_original_bytes(self):
        import layer4_platform
        import types
        spec = importlib.util.spec_from_file_location('fd_root_fixture_regression',
            Path(__file__).parents[1] / 'fixtures/layer4/browser_ops.py')
        fixture = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {'uninstall_ops': types.SimpleNamespace(pause_automatic_packages=Mock())}), \
                patch.object(layer4_platform, 'detected_guest', return_value='fedora'):
            spec.loader.exec_module(fixture)
        registrations = (
            '/usr/lib64/mozilla/native-messaging-hosts/blockuntu_native.json',
            '/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json',
            '/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json')
        with tempfile.TemporaryDirectory() as directory:
            sandbox = Path(directory)
            originals = {}
            for index, name in enumerate(registrations):
                path = sandbox / name.lstrip('/')
                path.parent.mkdir(parents=True, exist_ok=True)
                originals[name] = json.dumps({'name': 'blockuntu_native',
                    'path': '/usr/bin/blockuntu-native', 'type': 'stdio',
                    'independent_registration': index}, indent=2) + '\n'
                path.write_text(originals[name])
            fixture.LEASE = sandbox / 'lease.json'
            fixture.publish = Mock()
            fixture.run = Mock()
            killed = set()
            fixture.same_process = lambda row: row['pid'] not in killed
            fixture.inspect = lambda: {'files': {name: (sandbox / name.lstrip('/')).read_text()
                                                 for name in registrations},
                'daemon': {'pid': 500, 'state': 'S'},
                'native_processes': [{'pid': 501}, {'pid': 502}]}
            def scoped_path(name):
                return sandbox / str(name).lstrip('/') if str(name) in registrations else Path(name)
            def signal(pid, sig):
                self.assertIn(pid, (501, 502))
                self.assertEqual(sig, fixture.signal.SIGKILL)
                killed.add(pid)
            with patch.object(fixture, 'Path', scoped_path), patch.object(fixture.os, 'kill', signal):
                witness = fixture.fault('manifest')
                self.assertTrue(witness['fault_witnessed'])
                self.assertTrue(witness['connections_terminated'])
                self.assertEqual(killed, {501, 502})
                for name in registrations:
                    self.assertEqual(json.loads((sandbox / name.lstrip('/')).read_text())['path'],
                                     '/nonexistent/blockuntu-p43-native')
                fixture.restore()
            self.assertFalse(fixture.LEASE.exists())
            for name, data in originals.items():
                self.assertEqual((sandbox / name.lstrip('/')).read_text(), data)

    def test_native_polkit_lease_cannot_authorize_other_packages_or_commands(self):
        for guest in ('fedora', 'cachyos'):
            command = ' '.join(removal_argv(guest))
            script = '''let rule; const polkit={Result:{YES:true},addRule:r=>rule=r};
''' + ops.polkit_rule('/usr/bin/env', guest) + '''
const subject={user:'akhi',local:true,active:true};
const prefix='/usr/bin/env BLOCKUNTU_PACKAGE_REMOVAL_LEASE='+'a'.repeat(64)+' ';
function allowed(cmd,s=subject){return rule({id:'org.freedesktop.policykit.exec',lookup:k=>k==='program'?'/usr/bin/env':cmd},s)===true;}
const command=''' + repr(command) + ''';
if(!allowed(prefix+command) || allowed(prefix+command+'; reboot') ||
allowed(prefix+command.replace('blockuntu','other')) ||
allowed(prefix+command,{...subject,active:false}) ||
allowed(prefix+command.replace('remove','install').replace('-R','-S')))
process.exit(1);
'''
            answer = subprocess.run(['node'], input=script, capture_output=True, text=True)
            self.assertEqual(answer.returncode, 0, answer.stderr)

    def test_native_removal_requires_current_exact_version_transaction(self):
        now = dt.datetime.now().astimezone()
        for guest, event in [('cachyos', '[' + now.isoformat(timespec='seconds') + '] [ALPM] removed blockuntu (0.2.0-1)'),
                             ('fedora', now.isoformat(timespec='seconds') + ' INFO Erased: blockuntu-0.2.0-1.fc43.x86_64')]:
            version = '0.2.0-1.fc43' if guest == 'fedora' else '0.2.0-1'
            logs = [{'path': '/var/log/native.log', 'events': [event]}]
            self.assertTrue(native_removal_witness(logs, now.isoformat(), version, guest))
            self.assertIsNone(native_removal_witness(logs, now.isoformat(), 'wrong-version', guest))
            future = now + dt.timedelta(days=1)
            self.assertIsNone(native_removal_witness(logs, future.isoformat(), version, guest))
            logs[0]['events'] = [event.replace('removed', 'installed').replace('Erased', 'Installed')]
            self.assertIsNone(native_removal_witness(logs, now.isoformat(), version, guest))

    def test_dnf5_epoch_and_completed_callback_are_required(self):
        now = dt.datetime.now().astimezone().isoformat(timespec='seconds')
        package = '"blockuntu-0:0.2.0-1.fc43.x86_64"'
        for action, expected in [('RPM callback uninstall stop ', True),
                                 ('RPM callback uninstall start ', False),
                                 ('RPM callback install stop ', False),
                                 ('ERROR [rpm] erase failed ', False)]:
            logs = [{'path': '/var/log/dnf5.log', 'events': [now + ' [123] INFO ' + action + package]}]
            self.assertEqual(bool(native_removal_witness(logs, now, '0.2.0-1.fc43', 'fedora')), expected)

    def test_store_onboarding_focuses_actual_browser_before_permission_input(self):
        with tempfile.TemporaryDirectory() as directory:
            stubs = {name: Mock() for name in ('guest', 'browsers', 'ui_controls')}
            stubs['guest'].WORK = Path(directory)
            stubs['browsers'].APPS = {'firefox': 'Firefox'}
            stubs['browsers'].STORES = {'firefox': 'https://addons.mozilla.org/en-US/firefox/addon/blockuntu/'}
            def focused(app):
                self.assertEqual(app, 'Firefox')
                stubs['browsers'].start.assert_called_once()
                stubs['browsers'].confirm_install.assert_not_called()
                raise RuntimeError('native-focus-probe')
            stubs['ui_controls'].focus_app.side_effect = focused
            with patch.dict(sys.modules, stubs):
                spec = importlib.util.spec_from_file_location('onboard_focus_probe', Path(__file__).parents[1] / 'fixtures/layer4/uninstall.py')
                module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(module)
                with self.assertRaisesRegex(RuntimeError, 'native-focus-probe'):
                    module.onboard('firefox', 'focus-probe')
            stubs['ui_controls'].focus_app.assert_called_once_with('Firefox')

    def test_guest_pass_from_other_distro_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'guest'):
            verify_guest({'guest': 'ubuntu', 'scenario': 'remove', 'status': 'pass'}, 'remove', 'fedora')


class NativeArchiveAudit(unittest.TestCase):
    def native_batch(self, root, name, guest):
        summary_path = hosts_tests.HostsAuditTests().make_batch(root, name)
        summary = json.loads(summary_path.read_text())
        summary.update(guest=guest, scope=guest + '-p42')
        suffix = {'fedora': '.rpm', 'cachyos': '.pkg.tar.zst'}[guest]
        for row in summary['runs']:
            directory = Path(row['evidence'])
            result_path = directory / 'result.json'
            result = json.loads(result_path.read_text())
            result.update(guest=guest, scope=guest + '-p42-' + row['scenario'])
            result_path.write_text(json.dumps(result))
            report_path = directory / 'package-report.json'
            report = json.loads(report_path.read_text())
            report.update(component='rpm' if guest == 'fedora' else 'arch')
            report['artifacts'][0]['filename'] = 'blockuntu' + suffix
            report_path.write_text(json.dumps(report))
            owner_path = root / 'runtime/vm-phase0' / result['run_id'] / 'ubuntu/owner.json'
            owner = json.loads(owner_path.read_text())
            owner['template'] = guest
            target = owner_path.parent.parent / guest / 'owner.json'
            target.parent.mkdir()
            target.write_text(json.dumps(owner))
            owner_path.unlink()
        summary_path.write_text(json.dumps(summary))
        return summary_path

    def test_native_owned_archives_qualify_and_mixed_guests_do_not(self):
        for guest in ('fedora', 'cachyos'):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                a = self.native_batch(root, 'a', guest)
                b = self.native_batch(root, 'b', guest)
                with patch.object(hosts_auditor, 'ROOT', root), patch.object(hosts_auditor, 'verify_guest', side_effect=lambda result, scenario, expected: len(HOST_CASES[scenario])):
                    result = hosts_auditor.audit([a, b], True)
                    self.assertEqual(result['scope'], guest + '-p42')
                    changed = json.loads(b.read_text())
                    changed['guest'] = 'cachyos' if guest == 'fedora' else 'fedora'
                    b.write_text(json.dumps(changed))
                    with self.assertRaisesRegex(ValueError, 'mixed'):
                        hosts_auditor.audit([a, b], True)

    def test_qualification_rejects_mixed_rolling_fixture_preparation(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            a = self.native_batch(root, 'a', 'cachyos')
            b = self.native_batch(root, 'b', 'cachyos')
            summary = json.loads(b.read_text())
            summary['refresh_cachyos'] = True
            for row in summary['runs']:
                evidence = Path(row['evidence'])
                result_path = evidence / 'result.json'
                result = json.loads(result_path.read_text())
                result['refresh_cachyos'] = True
                result_path.write_text(json.dumps(result))
                for name in ('cachyos-baseline.log', 'os-update.log',
                             'pacman-cache-before-update.log', 'pacman-cache-after-update.log'):
                    (evidence / name).write_text('Recorded fresh fixture preparation\n')
            b.write_text(json.dumps(summary))
            with patch.object(hosts_auditor, 'ROOT', root), patch.object(hosts_auditor, 'verify_guest',
                    side_effect=lambda result, scenario, expected: len(HOST_CASES[scenario])):
                with self.assertRaisesRegex(ValueError, 'comparable rolling fixture'):
                    hosts_auditor.audit([a, b], True)


if __name__ == '__main__':
    unittest.main()
