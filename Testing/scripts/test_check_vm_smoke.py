"""Reject misleading repeatability claims from incomplete or mismatched evidence."""
from copy import deepcopy
import unittest

from check_vm_smoke import check_pair, check_run, unreadable_attempt
from layer3_contract import build_plan, load_contract


class SmokeEvidenceTests(unittest.TestCase):
    def setUp(self):
        # Synthetic evidence keeps these tests independent of ignored VM results.
        registry, matrix = load_contract()
        plan = build_plan(registry, matrix, guests=['ubuntu'])
        self.required = {(r['id'], r['browser_package'].removesuffix('-native')
                          if r['browser_package'] else None) for r in plan['observations']}
        self.expected = {'guest': 'ubuntu', 'commit': 'a' * 40, 'filename': 'blockuntu.deb',
                         'sha256': 'b' * 64, 'version': '0.2.0-1'}
        artifacts = [{'filename': 'blockuntu.deb', 'sha256': 'b' * 64}]
        self.package = {'passed': True, 'build_provenance_verified': True, 'commit': 'a' * 40,
                        'artifacts': artifacts, 'package_metadata': {'version': '0.2.0-1'}}
        self.manifest = {p: 'c' * 64 for p in ('scripts/vm-acceptance.py', 'scripts/phase0-vm.py',
            'fixtures/layer3/guest.py', 'fixtures/layer3/browsers.py',
            'fixtures/layer3/processes.py', 'fixtures/layer3/gui.py')}
        self.browsers = {browser: {'browser_version': 'test-version', 'extension_version': 'test-extension',
            'extension_id': browser + '-id', 'package_source': 'test source', 'policy_path': '/test/policy'}
            for browser in ('firefox', 'chrome')}
        cases = [{'id': cid, 'browser': browser, 'status': 'pass',
                  'evidence': {'heartbeat': {'extension_version': 'test-extension'}}}
                 for cid, browser in sorted(self.required, key=str)]
        next(row for row in cases if row['id'] == 'VM-INSTALL-001')['version'] = '0.2.0-1'
        self.result = {'run_id': 'test-a', 'guest': 'ubuntu', 'suite': 'ubuntu-smoke', 'status': 'pass',
            'base_integrity_verified': True, 'package_commit': 'a' * 40, 'package': artifacts,
            'harness_sha256': 'c' * 64, 'cases': cases,
            'started_at': '2026-09-27T10:00:00+00:00', 'finished_at': '2026-09-27T10:15:00+00:00'}
        self.owner = {'run_id': 'test-a', 'template': 'ubuntu', 'stage': 'removed',
                      'uuid': 'uuid-a', 'bases': {'ubuntu': 'unchanged'}}
        self.phase0 = {'status': 'passed', 'clone_uuid': 'uuid-a', 'machine_id': 'machine-a',
                       'host_fingerprint': 'fingerprint-a'}

    def checked(self):
        return check_run(self.result, self.package, self.manifest, self.browsers,
                         self.owner, self.phase0, self.expected, self.required)

    def pair(self):
        first = self.checked()
        second = deepcopy(first)
        second.update(run_id='test-b', clone_uuid='uuid-b', machine_id='machine-b',
                      host_fingerprint='fingerprint-b', started_at='2026-09-27T10:20:00+00:00',
                      finished_at='2026-09-27T10:35:00+00:00')
        return [first, second]

    def test_valid_evidence_and_comparable_fresh_pair(self):
        self.assertTrue(self.checked()['valid'])
        self.assertEqual(check_pair(self.pair())['status'], 'pass')

    def test_single_run_or_pending_attempt_cannot_complete_gate(self):
        self.assertEqual(check_pair([self.checked()])['status'], 'incomplete')
        self.assertEqual(check_pair(self.pair() + [{'started_at': None}])['status'], 'incomplete')

    def test_missing_or_duplicate_required_observation_rejected(self):
        self.result['cases'].pop()
        self.assertFalse(self.checked()['valid'])
        self.setUp()
        self.result['cases'].append(deepcopy(self.result['cases'][0]))
        self.assertFalse(self.checked()['valid'])

    def test_missing_browser_and_setup_failure_rejected(self):
        self.result['cases'] = [r for r in self.result['cases'] if r.get('browser') != 'chrome']
        self.assertFalse(self.checked()['valid'])
        self.setUp()
        self.result['cases'].append({'id': 'SETUP-GUEST', 'status': 'fail'})
        self.assertFalse(self.checked()['valid'])

    def test_package_commit_hash_and_installed_version_must_match(self):
        for mutation in ('commit', 'hash', 'version', 'provenance'):
            self.setUp()
            if mutation == 'commit':
                self.result['package_commit'] = 'd' * 40
            elif mutation == 'hash':
                self.package['artifacts'][0]['sha256'] = 'd' * 64
            elif mutation == 'version':
                next(r for r in self.result['cases'] if r['id'] == 'VM-INSTALL-001')['version'] = 'other'
            else:
                self.package['build_provenance_verified'] = False
            with self.subTest(mutation=mutation):
                self.assertFalse(self.checked()['valid'])

    def test_cleanup_and_base_integrity_are_required(self):
        self.owner['stage'] = 'stopped'
        self.assertFalse(self.checked()['valid'])
        self.setUp()
        self.result['base_integrity_verified'] = False
        self.assertFalse(self.checked()['valid'])

    def test_heartbeat_version_change_is_visible(self):
        row = next(r for r in self.result['cases'] if r['id'] == 'VM-BR-002' and r['browser'] == 'firefox')
        row['evidence']['heartbeat']['extension_version'] = 'new-version'
        self.assertFalse(self.checked()['valid'])

    def test_pair_versions_harness_package_and_bases_must_match(self):
        for field in ('browser_versions', 'manifest', 'package_sha256', 'bases'):
            pair = self.pair()
            pair[1][field] = 'changed'
            with self.subTest(field=field):
                self.assertEqual(check_pair(pair)['status'], 'fail')

    def test_fresh_clone_identities_cannot_be_reused(self):
        for field in ('run_id', 'clone_uuid', 'machine_id', 'host_fingerprint'):
            pair = self.pair()
            pair[1][field] = pair[0][field]
            with self.subTest(field=field):
                self.assertEqual(check_pair(pair)['status'], 'fail')

    def test_latest_failure_cannot_be_hidden_by_older_pair(self):
        runs = self.pair()
        failure = deepcopy(runs[-1])
        failure.update(run_id='test-c', started_at='2026-09-27T11:00:00+00:00', valid=False)
        self.assertEqual(check_pair(runs + [failure])['status'], 'fail')
        # An earlier, retained failure does not invalidate two later clean passes.
        failure['started_at'] = '2026-09-27T09:00:00+00:00'
        self.assertEqual(check_pair([failure] + runs)['status'], 'pass')

    def test_overlapping_runs_are_not_a_serial_pair(self):
        runs = self.pair()
        runs[1]['started_at'] = '2026-09-27T10:10:00+00:00'
        self.assertEqual(check_pair(runs)['status'], 'fail')

    def test_terminal_setup_failure_with_partial_bundle_remains_in_history(self):
        terminal = {**self.result, 'run_id': 'failed-setup', 'status': 'fail',
                    'started_at': '2026-09-27T09:00:00+00:00',
                    'finished_at': '2026-09-27T09:05:00+00:00'}
        failure = unreadable_attempt('failed-setup', 'ubuntu', 'No browser metadata', terminal)
        self.assertFalse(failure['valid'])
        self.assertEqual(check_pair([failure] + self.pair())['status'], 'pass')
        terminal.update(started_at='2026-09-27T11:00:00+00:00',
                        finished_at='2026-09-27T11:05:00+00:00')
        failure = unreadable_attempt('failed-setup', 'ubuntu', 'No browser metadata', terminal)
        self.assertEqual(check_pair(self.pair() + [failure])['status'], 'fail')

    def test_running_or_misattributed_partial_attempt_prevents_completion(self):
        for change in ({'status': 'running'}, {'run_id': 'different-run'}, {'finished_at': 'invalid'}):
            result = {**self.result, **change}
            incomplete = unreadable_attempt('test-a', 'ubuntu', 'Partial bundle', result)
            with self.subTest(change=change):
                self.assertIsNone(incomplete['started_at'])
                self.assertEqual(check_pair(self.pair() + [incomplete])['status'], 'incomplete')


if __name__ == '__main__':
    unittest.main()
