"""Group runner must fail before cloning and stop on incomplete evidence."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from layer3_group_runner import execute, isolated_assertion_valid, preflight, verify_bundle
from harness_fixtures import Layer3Fixture, write_json


class GroupRunnerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.fixture = Layer3Fixture(temporary.name)
        self.root, self.inputs = self.fixture.root, self.fixture.inputs

    def prepare(self, *args, **kwargs):
        return preflight(*args, root=self.root, **kwargs)

    def checked_bundle(self, prepared, index=0):
        self.fixture.bundle(prepared, index)
        item = prepared['runs'][index]
        return verify_bundle(item, prepared['expected'][item['guest']],
                             prepared['plan'], root=self.root)

    def test_exact_smoke_preflight_and_fixture_evidence(self):
        prepared = self.prepare('smoke', ['ubuntu'], 'p31c-test-unused', self.inputs)
        report = Path(prepared['expected']['ubuntu']['report'])
        self.assertEqual(report.parent.name, 'debian')
        self.assertEqual(json.loads(report.read_text())['os_release'], 'ID=debian\n')
        self.assertEqual(len(prepared['runs']), 1)
        self.assertEqual(prepared['commit'], self.fixture.commit)
        checked = self.checked_bundle(prepared)
        self.assertTrue(checked['valid'], checked['errors'])
        self.assertEqual(checked['required_passes'], 21)

    def test_ubuntu_isolated_case_preflight(self):
        prepared = self.prepare('core', ['ubuntu'], 'p31w-test-unused', self.inputs,
                             case_ids=['VM-WAL-001'])
        self.assertEqual(prepared['groups']['runnable_group_count'], 1)
        self.assertEqual(prepared['runs'][0]['observation_ids'],
                         ['ubuntu:VM-WAL-001:none:baseline'])
        with self.assertRaisesRegex(ValueError, 'without an implemented executor'):
            self.prepare('core', ['fedora'], 'p31w-test-unused', self.inputs,
                      case_ids=['VM-WAL-001'])

    def test_p32_ubuntu_matcher_group_preflight_and_command(self):
        cases = ['VM-WEB-003', 'VM-WEB-004', 'VM-WEB-005', 'VM-WEB-006', 'VM-WEB-007']
        prepared = self.prepare('core', ['ubuntu'], 'p32web-test-unused', self.inputs,
                             case_ids=cases)
        self.assertEqual(len(prepared['runs']), 1)
        self.assertEqual(len(prepared['runs'][0]['observation_ids']), 10)
        self.assertEqual(prepared['runs'][0]['case_ids'], cases)
        with tempfile.TemporaryDirectory() as directory:
            prepared['output_dir'] = Path(directory) / 'group-run'
            commands = []
            def invoke(command, **kwargs):
                commands.append(command)
                return subprocess.CompletedProcess(command, 0)
            def checker(*args, **kwargs):
                return {'valid': True, 'errors': []}
            _, summary = execute(prepared, root=Path(directory), invoke=invoke, checker=checker)
            self.assertEqual(summary['status'], 'pass')
            self.assertEqual([commands[0][index + 1] for index, value in enumerate(commands[0])
                              if value == '--case'], cases)

    def test_p32_ubuntu_domain_group_preflight(self):
        prepared = self.prepare('core', ['ubuntu'], 'p32domain-test-unused', self.inputs,
                             case_ids=['VM-WEB-001'])
        self.assertEqual(len(prepared['runs']), 1)
        self.assertEqual(len(prepared['runs'][0]['observation_ids']), 2)
        self.assertEqual(prepared['runs'][0]['group_id'], 'ubuntu:p32-native-domain')
        self.assertEqual(prepared['runs'][0]['case_ids'], ['VM-WEB-001'])

    def test_p32_ubuntu_domain_requires_dns_and_browser_evidence(self):
        prepared = self.prepare('core', ['ubuntu'], 'p32domain-test-unused', self.inputs,
                             case_ids=['VM-WEB-001'])
        checked = self.checked_bundle(prepared)
        self.assertTrue(checked['valid'], checked['errors'])
        self.assertEqual(checked['required_passes'], 2)
        self.assertEqual(checked['smoke_prerequisite_passes'], 21)

    def test_p32_failed_navigation_cannot_be_promoted_after_clone_cleanup(self):
        cases = ['VM-WEB-003', 'VM-WEB-004', 'VM-WEB-005', 'VM-WEB-006', 'VM-WEB-007']
        prepared = self.prepare('core', ['ubuntu'], 'p32web-test-unused', self.inputs,
                             case_ids=cases)
        directory = self.fixture.bundle(prepared)
        result = json.loads((directory / 'result.json').read_text())
        row = next(row for row in result['observations']
                   if row['id'] == 'VM-WEB-007' and row['browser_package'] == 'firefox-native')
        row['evidence']['history_navigation_blocked'] = False
        write_json(directory / 'result.json', result)
        checked = verify_bundle(prepared['runs'][0], prepared['expected']['ubuntu'],
                                prepared['plan'], root=self.root)
        self.assertFalse(checked['valid'])
        self.assertTrue(any('VM-WEB-007:firefox-native' in error for error in checked['errors']))

    def test_p32_ubuntu_group_has_complete_separate_evidence(self):
        cases = ['VM-WEB-003', 'VM-WEB-004', 'VM-WEB-005', 'VM-WEB-006', 'VM-WEB-007']
        prepared = self.prepare('core', ['ubuntu'], 'p32web-test-unused', self.inputs,
                             case_ids=cases)
        checked = self.checked_bundle(prepared)
        self.assertTrue(checked['valid'], checked['errors'])
        self.assertEqual(checked['required_passes'], 10)
        self.assertEqual(checked['smoke_prerequisite_passes'], 21)

    def test_p32_rejects_missing_screenshot_or_changed_checkpoint(self):
        cases = ['VM-WEB-003', 'VM-WEB-004', 'VM-WEB-005', 'VM-WEB-006', 'VM-WEB-007']
        prepared = self.prepare('core', ['ubuntu'], 'fixture-web', self.inputs, case_ids=cases)
        for fault in ('screenshot', 'checkpoint'):
            with self.subTest(fault=fault):
                directory = self.fixture.bundle(prepared)
                if fault == 'screenshot':
                    (directory / 'firefox-p32-spa.png').unlink()
                else:
                    write_json(directory / 'p32-result.json', [])
                checked = verify_bundle(prepared['runs'][0], prepared['expected']['ubuntu'],
                                        prepared['plan'], root=self.root)
                self.assertFalse(checked['valid'])

    def test_domain_bundle_rejects_missing_forwarding_witness(self):
        prepared = self.prepare('core', ['ubuntu'], 'fixture-dns', self.inputs,
                                case_ids=['VM-WEB-001'])
        directory = self.fixture.bundle(prepared)
        dns = json.loads((directory / 'domain-dns.json').read_text())
        dns['public_query_resolved'] = False
        write_json(directory / 'domain-dns.json', dns)
        checked = verify_bundle(prepared['runs'][0], prepared['expected']['ubuntu'],
                                prepared['plan'], root=self.root)
        self.assertFalse(checked['valid'])
        self.assertIn('Guest-local DNS baseline/forwarding evidence is incomplete', checked['errors'])

    def test_smoke_bundle_rejects_unremoved_clone_or_missing_observation(self):
        prepared = self.prepare('smoke', ['ubuntu'], 'fixture-smoke', self.inputs)
        for fault in ('owner', 'observation'):
            with self.subTest(fault=fault):
                directory = self.fixture.bundle(prepared)
                path = (self.root / 'runtime/vm-phase0/fixture-smoke-ubuntu/ubuntu/owner.json'
                        if fault == 'owner' else directory / 'result.json')
                data = json.loads(path.read_text())
                if fault == 'owner':
                    data['stage'] = 'stopped'
                else:
                    data['observations'].pop()
                write_json(path, data)
                checked = verify_bundle(prepared['runs'][0], prepared['expected']['ubuntu'],
                                        prepared['plan'], root=self.root)
                self.assertFalse(checked['valid'])

    def test_ubuntu_isolated_case_has_complete_evidence(self):
        prepared = self.prepare('core', ['ubuntu'], 'p31w-test-unused', self.inputs,
                             case_ids=['VM-WAL-001'])
        checked = self.checked_bundle(prepared)
        self.assertTrue(checked['valid'], checked['errors'])
        self.assertEqual(checked['required_passes'], 1)

    def test_two_case_batch_has_complete_separate_evidence(self):
        prepared = self.prepare('core', ['ubuntu'], 'p31batch-unused', self.inputs,
                             case_ids=['VM-AAL-001', 'VM-WAL-005'])
        for index, item in enumerate(prepared['runs']):
            with self.subTest(case=item['case_id']):
                checked = self.checked_bundle(prepared, index)
                self.assertTrue(checked['valid'], checked['errors'])
                self.assertEqual(checked['required_passes'], 1)

    def test_two_isolated_cases_get_distinct_ids_and_commands(self):
        prepared = self.prepare('core', ['ubuntu'], 'p31batch-unused', self.inputs,
                             case_ids=['VM-AAL-001', 'VM-WAL-005'])
        self.assertEqual(len(prepared['runs']), 2)
        self.assertEqual(len({row['run_id'] for row in prepared['runs']}), 2)
        self.assertEqual({row['case_id'] for row in prepared['runs']},
                         {'VM-AAL-001', 'VM-WAL-005'})
        with tempfile.TemporaryDirectory() as directory:
            prepared['output_dir'] = Path(directory) / 'group-run'
            commands = []
            def invoke(command, **kwargs):
                commands.append(command)
                return subprocess.CompletedProcess(command, 0)
            def checker(*args, **kwargs):
                return {'valid': True, 'errors': []}
            _, summary = execute(prepared, root=Path(directory), invoke=invoke, checker=checker)
            self.assertEqual(summary['status'], 'pass')
            self.assertEqual([command[command.index('--case') + 1] for command in commands],
                             [row['case_id'] for row in prepared['runs']])

    def test_isolated_assertions_require_case_specific_evidence(self):
        common = {'enforcement_active': True}
        self.assertFalse(isolated_assertion_valid('VM-WAL-005',
            {**common, 'policy_unchanged': True}))
        self.assertFalse(isolated_assertion_valid('VM-AAL-001',
            {**common, 'policy_unchanged': True, 'rejection': 'unrelated'}))

    def test_unimplemented_suite_refused_before_artifact_verification(self):
        with self.assertRaisesRegex(ValueError, 'without an implemented executor'):
            self.prepare('full', ['ubuntu'], 'p31c-test-unused', Path('/missing/inputs'))

    def test_used_run_id_refused(self):
        existing = self.root / 'results/p31c-test-used-ubuntu'
        existing.mkdir(exist_ok=False)
        try:
            with self.assertRaises(FileExistsError):
                self.prepare('smoke', ['ubuntu'], 'p31c-test-used', self.inputs)
        finally:
            existing.rmdir()

    def test_wrong_package_record_refused(self):
        inputs = json.loads(self.inputs.read_text())
        inputs['packages'][0]['sha256'] = '0' * 64
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'inputs.json'
            path.write_text(json.dumps(inputs))
            with self.assertRaisesRegex(ValueError, 'differs from the exact ubuntu package'):
                self.prepare('smoke', ['ubuntu'], 'p31c-test-unused', path)

    def test_serial_execution_stops_and_keeps_summary(self):
        prepared = self.prepare('smoke', ['ubuntu', 'fedora', 'cachyos'],
                             'p31c-test-unused', self.inputs)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prepared['output_dir'] = root / 'group-run'
            calls = []

            def invoke(command, **kwargs):
                calls.append(command)
                return subprocess.CompletedProcess(command, 1 if len(calls) == 2 else 0)

            def checker(item, expected, plan, **kwargs):
                return {'valid': len(calls) == 1, 'errors': [] if len(calls) == 1 else ['bad evidence']}

            path, summary = execute(prepared, root=root, invoke=invoke, checker=checker)
            self.assertEqual(summary['status'], 'fail')
            self.assertEqual([row['status'] for row in summary['runs']],
                             ['pass', 'fail', 'not_run'])
            self.assertEqual(len(calls), 2)
            self.assertEqual(json.loads(path.read_text())['runs'][2]['status'], 'not_run')
            self.assertEqual(path.parent.stat().st_mode & 0o777, 0o700)
            self.assertEqual(summary['full_layer3_acceptance'], False)

    def test_zero_exit_without_result_is_failure(self):
        prepared = self.prepare('smoke', ['ubuntu'], 'p31c-test-unused', self.inputs)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prepared['output_dir'] = root / 'group-run'
            def invoke(command, **kwargs):
                return subprocess.CompletedProcess(command, 0)
            path, summary = execute(prepared, root=root, invoke=invoke)
            self.assertEqual(summary['status'], 'fail')
            self.assertIn('No such file', summary['runs'][0]['error'])
            self.assertTrue(path.exists())


if __name__ == '__main__':
    unittest.main()
