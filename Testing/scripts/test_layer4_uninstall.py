"""Regression checks for the incremental P4.0 trust/evidence boundaries."""
import importlib.util
import datetime as dt
import gzip
import json
import sys
from pathlib import Path
import shutil
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from layer4_uninstall_contract import CASES, finalize, firefox_sentinel_preservation, input_keys, native_purge_witness, redact, save, validate_action, verify_guest

spec = importlib.util.spec_from_file_location('uninstall_runner', Path(__file__).with_name('uninstall-acceptance.py'))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
audit_spec = importlib.util.spec_from_file_location('uninstall_audit', Path(__file__).with_name('check_uninstall_acceptance.py'))
auditor = importlib.util.module_from_spec(audit_spec)
audit_spec.loader.exec_module(auditor)
ops_spec = importlib.util.spec_from_file_location('uninstall_ops', runner.ROOT / 'fixtures/layer4/uninstall_ops.py')
ops = importlib.util.module_from_spec(ops_spec)
ops_spec.loader.exec_module(ops)


class ActionBoundaryTests(unittest.TestCase):
    def test_package_lock_wait_never_deletes_lock_or_interrupts_owner(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / 'dpkg-lock'
            p.write_text('sentinel')
            code = 'import fcntl,sys; f=open(sys.argv[1],"r+"); fcntl.lockf(f,fcntl.LOCK_EX); print("ready",flush=True); input()'
            owner = subprocess.Popen([sys.executable, '-c', code, str(p)], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, text=True)
            try:
                self.assertEqual(owner.stdout.readline().strip(), 'ready')
                with self.assertRaisesRegex(TimeoutError, 'transaction'):
                    ops.wait_package_locks([p], seconds=0.03, poll=0.005)
                self.assertIsNone(owner.poll())
                self.assertEqual(p.read_text(), 'sentinel')
                owner.communicate('\n',timeout=5)
                ops.wait_package_locks([p],seconds=1,poll=0.005)
                self.assertEqual(p.read_text(),'sentinel')
            finally:
                if owner.poll() is None:
                    owner.communicate('\n',timeout=5)

    @unittest.skipUnless(shutil.which('node'), 'JavaScript runtime needed to verify Polkit rule')
    def test_polkit_rule_handles_resolved_env_but_rejects_other_commands_and_subjects(self):
        setup = 'var rule; var polkit = {Result:{YES:"YES"},addRule:function(r){rule=r;}};\n'
        script = '''
var subject={user:"akhi",local:true,active:true};
var prefix="/usr/bin/env BLOCKUNTU_PACKAGE_REMOVAL_LEASE="+"a".repeat(64);
function check(program,command,s) {
 return rule({id:"org.freedesktop.policykit.exec",lookup:function(k){return k==="program"?program:command;}},s||subject)==="YES";
}
var cases=[
 check("/usr/lib/cargo/bin/coreutils/env",prefix+" /usr/bin/dpkg --purge blockuntu"),
 check("/usr/bin/env",prefix+" /usr/bin/dpkg --purge blockuntu"),
 !check("/usr/bin/bash",prefix+" /usr/bin/dpkg --purge blockuntu"),
 !check("/usr/bin/env",prefix+" /usr/bin/dpkg --purge other-package"),
 !check("/usr/bin/env",prefix+" /usr/bin/dpkg --purge blockuntu; reboot"),
 !check("/usr/bin/env","/usr/bin/env BLOCKUNTU_PACKAGE_REMOVAL_LEASE=invalid /usr/bin/dpkg --purge blockuntu"),
 !check("/usr/bin/env",prefix+" /usr/bin/dpkg --purge blockuntu",{user:"root",local:true,active:true}),
 !check("/usr/bin/env",prefix+" /usr/bin/dpkg --purge blockuntu",{user:"akhi",local:true,active:false})
];
if(!cases.every(Boolean)) process.exit(1);
'''
        answer = subprocess.run(['node'], input=setup+ops.polkit_rule('/usr/lib/cargo/bin/coreutils/env')+script,
                                text=True,capture_output=True,timeout=10)
        self.assertEqual(answer.returncode,0,answer.stderr)

    def request(self, action, **kwargs):
        return {'id': 'a' * 32, 'action': action, **kwargs}

    def test_command_path_and_credential_payloads_are_rejected(self):
        bad = [self.request('fault', name='direct-remove; reboot'),
               self.request('fault', name='direct-remove', command='id'),
               self.request('type_credential', slot='uninstall', value='secret'),
               self.request('screenshot', name='../base'),
               self.request('checkpoint', name='/etc/hosts'),
               self.request('type_url', url='file:///etc/blockuntu/uninstall-recovery.txt'),
               self.request('keys', keys=['KEY_LEFTCTRL', 'KEY_LEFTALT', 'KEY_DELETE'])]
        for action in bad:
            with self.subTest(action=action), self.assertRaises(ValueError):
                validate_action(action, 'authorized')

    def test_independent_clone_scenarios_cannot_inject_other_faults(self):
        validate_action(self.request('fault', name='direct-remove'), 'remove')
        for scenario, action in [('purge', self.request('fault', name='direct-remove')),
                                 ('authorized', self.request('fault', name='direct-remove')),
                                 ('remove', self.request('type_credential', slot='uninstall'))]:
            with self.assertRaises(ValueError):
                validate_action(action, scenario)

    def test_native_browser_tab_selection_used_by_shared_adapter_is_permitted(self):
        validate_action(self.request('keys', keys=['KEY_LEFTCTRL', 'KEY_2']), 'authorized')

    def test_gui_focus_has_no_guest_supplied_target_or_key_payload(self):
        action = self.request('focus_gui')
        validate_action(action, 'authorized')
        for bad, scenario in ((action, 'remove'), (action, 'purge'),
                              (self.request('focus_gui', target='Ubuntu'), 'authorized'),
                              (self.request('focus_gui', keys=['KEY_DELETE']), 'authorized')):
            with self.assertRaises(ValueError):
                validate_action(bad, scenario)
        with patch.object(runner.vm, 'guarded') as guard, patch.object(runner.vm, 'virsh') as virsh, \
                patch.object(runner, 'checked'):
            runner.handle_action({'uuid': 'owned-clone'}, Path('/unused'), action, 'authorized')
        guard.assert_called_once()
        virsh.assert_called_once_with('send-key', 'owned-clone', '--codeset', 'linux', '--holdtime', '100',
                                     'KEY_LEFTALT', 'KEY_TAB', 'KEY_RIGHT')

    def test_credential_keyboard_encoding_is_bounded(self):
        self.assertTrue(input_keys('BLOCKUNTU-UNINSTALL-RECOVERY-AB01'))
        for value in ('BLOCKUNTU-TIER1-EDIT-A', 'password', 'BLOCKUNTU-UNINSTALL-RECOVERY-$(id)'):
            with self.assertRaises(ValueError):
                input_keys(value)

    def test_clone_guard_runs_before_guest_command(self):
        vm = SimpleNamespace(guarded=Mock(side_effect=ValueError('base UUID')), ssh=Mock())
        with patch.object(runner, 'vm', vm), self.assertRaisesRegex(ValueError, 'base UUID'):
            runner.checked({}, 'sudo -n apt-get remove blockuntu')
        vm.ssh.assert_not_called()

    def test_malformed_fault_never_reaches_guard_or_transport(self):
        vm = SimpleNamespace(guarded=Mock(), ssh=Mock())
        with patch.object(runner, 'vm', vm), self.assertRaises(ValueError):
            runner.handle_action({}, Path('/unused'), self.request('fault', name='direct-purge'), 'remove')
        vm.guarded.assert_not_called()
        vm.ssh.assert_not_called()


class EvidenceTests(unittest.TestCase):
    def test_firefox_preservation_requires_immediate_pre_uninstall_sentinel(self):
        sentinel = json.dumps({'policies': {'DisableTelemetry': True}})
        absent = json.dumps({'policies': {'BlockAboutConfig': True}})
        self.assertEqual(firefox_sentinel_preservation(sentinel, sentinel)['status'], 'preserved')
        self.assertEqual(firefox_sentinel_preservation(sentinel, None)['status'], 'lost_during_uninstall')
        for after in (sentinel, None):
            observed = firefox_sentinel_preservation(absent, after)
            self.assertEqual(observed['status'], 'not_covered')
            self.assertFalse(observed['covered'])
        with self.assertRaises(ValueError):
            firefox_sentinel_preservation('invalid json', sentinel)

    def test_rotated_native_purge_requires_current_time_and_candidate_version(self):
        now = dt.datetime.now().astimezone()
        stamp = now.strftime('%Y-%m-%d %H:%M:%S')
        old = (now-dt.timedelta(days=1)).strftime('%Y-%m-%d %H:%M:%S')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'dpkg.log').write_text('')
            (root/'dpkg.log.1').write_text(stamp+' purge blockuntu:amd64 0.2.0-1 <none>\n'+
                                         stamp+' status not-installed blockuntu:amd64 <none>\n')
            with gzip.open(root/'dpkg.log.2.gz','wt') as stream:
                stream.write(old+' purge blockuntu:amd64 0.2.0-1 <none>\n'+
                             old+' status not-installed blockuntu:amd64 <none>\n')
            logs=ops.dpkg_logs(root)
            witness=native_purge_witness(logs,now.isoformat(),'0.2.0-1')
            self.assertIsNotNone(witness)
            self.assertTrue(all(r['path'].endswith('.1') for rows in witness.values() for r in rows))
            self.assertIsNone(native_purge_witness(logs,now.isoformat(),'0.3.0-1'))
            self.assertIsNone(native_purge_witness([logs[-1]],now.isoformat(),'0.2.0-1'))

    def test_credentials_and_compact_lease_json_are_redacted_but_hashes_survive(self):
        token = 'a' * 64
        value = 'BLOCKUNTU-UNINSTALL-RECOVERY-A12 BLOCKUNTU_PACKAGE_REMOVAL_LEASE=' + token
        for text in (value, '{"package_removal_lease":"' + token + '"}', '{"lease_token": "' + token + '"}'):
            self.assertNotIn(token, redact(text))
            self.assertNotIn('BLOCKUNTU-UNINSTALL-RECOVERY-A12', redact(text))
        self.assertIn(token, redact(json.dumps({'sha256': token})))

    def test_atomic_owner_only_checkpoint(self):
        with tempfile.TemporaryDirectory() as temporary:
            p = Path(temporary) / 'result.json'
            save(p, {'status': 'running', 'secret': 'BLOCKUNTU-TIER1-EDIT-A1'})
            save(p, {'status': 'fail'})
            self.assertEqual(json.loads(p.read_text()), {'status': 'fail'})
            self.assertEqual(p.stat().st_mode & 0o777, 0o600)
            self.assertFalse(p.with_suffix('.json.tmp').exists())

    def test_checkpoint_temp_symlink_cannot_overwrite_another_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            p = Path(temporary) / 'result.json'
            target = Path(temporary) / 'protected'
            target.write_text('unchanged')
            p.with_suffix('.json.tmp').symlink_to(target)
            with self.assertRaises(OSError):
                save(p, {'status': 'pass'})
            self.assertEqual(target.read_text(), 'unchanged')

    def test_claimed_pass_with_missing_or_reordered_observations_is_rejected(self):
        result = {'status': 'pass', 'scenario': 'remove', 'guest': 'ubuntu', 'ordinary_uid': 1000,
                  'cases': [{'id': 'DES-UNINSTALL-001', 'status': 'pass', 'evidence': {}}]}
        with self.assertRaises(ValueError):
            verify_guest(result, 'remove')
        result['cases'] = [{'id': cid, 'variant': 'remove', 'status': 'pass', 'evidence': {}} for cid in CASES['remove']]
        with self.assertRaisesRegex(ValueError, 'Incomplete evidence'):
            verify_guest(result, 'remove')

    def test_other_uninstall_variant_cannot_be_accepted(self):
        result = {'status': 'pass', 'scenario': 'remove', 'guest': 'ubuntu', 'ordinary_uid': 1000,
                  'cases': [{'id': cid, 'variant': 'purge', 'status': 'pass', 'evidence': {}}
                            for cid in CASES['remove']]}
        with self.assertRaisesRegex(ValueError, 'variant'):
            verify_guest(result, 'remove')

    def test_missing_preservation_and_browser_evidence_cannot_be_hidden_by_status(self):
        result = {'status': 'pass', 'scenario': 'authorized', 'guest': 'ubuntu', 'ordinary_uid': 1000,
                  'baseline_browser_count': 2, 'cases': [
                      {'id': cid, 'status': 'pass', 'evidence': {}} for cid in CASES['authorized']]}
        with self.assertRaises(ValueError):
            verify_guest(result, 'authorized')


class RetentionTests(unittest.TestCase):
    def fixtures(self):
        return SimpleNamespace(unchanged=Mock(), cleanup=Mock()), SimpleNamespace(stop=Mock())

    def test_failed_or_incomplete_evidence_retains_stopped_clone(self):
        for result in ({'status': 'fail'}, {'status': 'pass', 'collection_errors': [{'file': 'journal.log'}]}):
            vm, suite = self.fixtures()
            finalize(vm, suite, {}, result)
            vm.cleanup.assert_not_called()
            suite.stop.assert_called_once()
            self.assertEqual(result['status'], 'fail')
            self.assertTrue(result['clone_retained'] and result['clone_stopped'])

    def test_success_requires_cleanup_and_base_comparison(self):
        vm, suite = self.fixtures()
        result = {'status': 'pass'}
        finalize(vm, suite, {}, result)
        vm.cleanup.assert_called_once()
        self.assertEqual(vm.unchanged.call_count, 2)
        self.assertTrue(result['clone_removed'] and result['base_integrity_verified'])

    def test_base_or_cleanup_failure_invalidates_product_pass(self):
        for failing in ('stop', 'unchanged', 'cleanup'):
            vm, suite = self.fixtures()
            getattr(suite if failing == 'stop' else vm, failing).side_effect = ValueError('unsafe')
            result = {'status': 'pass'}
            finalize(vm, suite, {}, result)
            self.assertEqual(result['status'], 'fail')
            self.assertIn('safety_error', result)
            self.assertFalse(result.get('clone_removed', False))


class CheckpointRaceTests(unittest.TestCase):
    def test_terminal_publication_between_read_and_service_exit_is_reread(self):
        with tempfile.TemporaryDirectory() as directory:
            reads = []
            terminal = {'status': 'pass', 'cases': []}
            def checked(record, command, **kwargs):
                if 'p41-result.json' in command:
                    reads.append(command)
                    return SimpleNamespace(returncode=0, stdout=json.dumps(
                        {'status': 'running', 'cases': []} if len(reads) == 1 else terminal))
                if 'is-active' in command:
                    return SimpleNamespace(returncode=3, stdout='inactive')
                return SimpleNamespace(returncode=1, stdout='')
            with patch.object(runner, 'checked', checked):
                self.assertEqual(runner.exercise({'run_id': 'fixture', 'template': 'ubuntu'}, Path(directory), 'remove'), terminal)

    def test_exit_with_only_partial_checkpoint_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            def checked(record, command, **kwargs):
                if 'p41-result.json' in command:
                    return SimpleNamespace(returncode=0, stdout=json.dumps({'status': 'running', 'cases': []}))
                if 'is-active' in command:
                    return SimpleNamespace(returncode=3, stdout='inactive')
                return SimpleNamespace(returncode=1, stdout='')
            with patch.object(runner, 'checked', checked), self.assertRaisesRegex(RuntimeError, 'terminal checkpoint'):
                runner.exercise({'run_id': 'fixture', 'template': 'ubuntu'}, Path(directory), 'remove')

    def test_slot_completion_never_contains_the_typed_credential(self):
        replies = []
        def checked(record, command, **kwargs):
            if 'host-action-done.json' in command:
                replies.append(json.loads(kwargs['input']))
            return SimpleNamespace(returncode=0, stdout='')
        action = {'id': 'a' * 32, 'action': 'type_credential', 'slot': 'invalid'}
        with patch.object(runner, 'checked', checked), patch.object(runner.vm, 'guarded'), \
                patch.object(runner.vm, 'virsh'), patch.object(runner.time, 'sleep'):
            runner.handle_action({'uuid': 'fixture-clone'}, Path('/unused'), action, 'authorized')
        self.assertEqual(replies, [{'id': action['id'], 'typed_characters': len('BLOCKUNTU-UNINSTALL-RECOVERY-INVALID')}])
        self.assertNotIn('BLOCKUNTU-', json.dumps(replies))


class AuditBoundaryTests(unittest.TestCase):
    def test_single_candidate_pass_cannot_claim_adapter_qualification(self):
        with self.assertRaisesRegex(ValueError, 'exactly two'):
            auditor.audit([Path('/unused')], qualify=True)

    def summary(self, path, scenarios=None, status='pass'):
        path.write_text(json.dumps({'status': status, 'guest': 'ubuntu', 'selected_scenarios': scenarios or list(CASES),
                                    'runs': [{'scenario': s, 'evidence': '/etc/acceptance'} for s in list(CASES)]}))

    def test_failed_or_partial_batch_cannot_pass_audit(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / 'summary.json'
            for scenarios, status in [(['remove'], 'pass'), (list(CASES), 'running'), (list(CASES), 'fail')]:
                self.summary(p, scenarios, status)
                with self.assertRaises(ValueError):
                    auditor.audit([p])

    def test_evidence_paths_cannot_escape_results_tree(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / 'summary.json'
            self.summary(p)
            with self.assertRaisesRegex(ValueError, 'outside'):
                auditor.audit([p])


if __name__ == '__main__':
    unittest.main()
