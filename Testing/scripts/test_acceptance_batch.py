"""Continue real suite failures without accepting wrong artifacts or unsafe VMs."""
import copy
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import acceptance_batch as batch


class FakeBackend:
    def __init__(self, codes, *, audit_code=0, unsafe_cleanup=False, unsafe_check=None):
        self.codes = iter(codes)
        self.events = []
        self.states = []
        self.audit_code = audit_code
        self.unsafe_cleanup = unsafe_cleanup
        self.unsafe_check = unsafe_check
        self.checks = 0

    def publish(self, state): self.states.append(copy.deepcopy(state))
    def stop_requested(self): return False
    def check(self):
        self.checks += 1
        if self.checks == self.unsafe_check: raise batch.SafetyError('Base changed')
    def invoke(self, task, row):
        self.events.append(('run', task['name']))
        return next(self.codes)
    def inspect(self, task): return {'status': 'pass'}
    def audit(self, task):
        self.events.append(('audit', task['name']))
        return self.audit_code
    def cleanup(self, task):
        self.events.append(('cleanup', task['name']))
        if self.unsafe_cleanup: raise batch.SafetyError('Clone guard rejected cleanup')


class BatchTests(unittest.TestCase):
    def tasks(self): return [{'name': 'first'}, {'name': 'second'}]

    def test_nonzero_suite_is_preserved_and_next_suite_runs_after_cleanup(self):
        backend = FakeBackend([1, 0])
        state = batch.execute_plan(self.tasks(), backend, continue_on_failure=True)
        self.assertEqual([r['status'] for r in state['commands']], ['fail', 'pass'])
        self.assertEqual(state['status'], 'completed_with_failures')
        self.assertEqual(state['failure_count'], 1)
        self.assertEqual(backend.events, [('run', 'first'), ('cleanup', 'first'),
                                          ('run', 'second'), ('cleanup', 'second')])

    def test_default_still_stops_on_first_suite_failure(self):
        state = batch.execute_plan(self.tasks(), FakeBackend([1, 0]))
        self.assertEqual(state['status'], 'stopped_on_failure')
        self.assertEqual(len(state['commands']), 1)

    def test_audit_failure_is_a_failure_and_next_independent_suite_runs(self):
        tasks = self.tasks(); tasks[0]['audit'] = {'command': ['fixture-audit']}
        backend = FakeBackend([0, 0], audit_code=1)
        state = batch.execute_plan(tasks, backend, continue_on_failure=True)
        self.assertEqual([r['status'] for r in state['commands']], ['fail', 'pass'])
        self.assertEqual(state['commands'][0]['audit_exit_code'], 1)

    def test_successful_process_cannot_hide_failed_acceptance_evidence(self):
        backend = FakeBackend([0, 0]); backend.inspect = lambda task: {'status': 'fail'}
        state = batch.execute_plan(self.tasks(), backend, continue_on_failure=True)
        self.assertEqual(state['status'], 'completed_with_failures')
        self.assertEqual(state['failure_count'], 2)

    def test_cleanup_guard_failure_is_always_a_hard_stop(self):
        backend = FakeBackend([1, 0], unsafe_cleanup=True)
        state = batch.execute_plan(self.tasks(), backend, continue_on_failure=True)
        self.assertEqual(state['status'], 'stopped_for_safety')
        self.assertEqual(len(state['commands']), 1)
        self.assertEqual(state['commands'][0]['status'], 'fail')
        self.assertNotIn(('run', 'second'), backend.events)

    def test_base_change_after_a_pass_stops_without_reclassifying_that_pass(self):
        state = batch.execute_plan(self.tasks(), FakeBackend([0, 0], unsafe_check=2),
                                   continue_on_failure=True)
        self.assertEqual(state['status'], 'stopped_for_safety')
        self.assertEqual(state['commands'][0]['status'], 'pass')
        self.assertEqual(len(state['commands']), 1)

    def test_same_candidate_native_report_is_required_before_starting_any_vm(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            spec = {'commit': 'a' * 40, 'package_reports': {'debian': root / 'new-debian.json'}}
            task = {'name': 'ubuntu-tier1', 'layer': 4, 'guest': 'ubuntu', 'run_id': 'fresh-ub-tier1',
                    'report': str(batch.RESULTS / 'fresh-ub-tier1/ubuntu/acceptance/result.json'),
                    'command': ['python3', str(root / 'run-acceptance.py'), 'tier1-edit-acceptance.py',
                                '--guest', 'ubuntu', '--run-id', 'fresh-ub-tier1',
                                '--package-report', str(root / 'old-debian.json')]}
            with self.assertRaisesRegex(ValueError, 'Layer 4 package report differs'):
                batch.validate_plan([task], spec, root)
            task['command'][-1] = str(spec['package_reports']['debian'])
            batch.validate_plan([task], spec, root)

    def test_report_provenance_mismatch_is_a_safety_stop(self):
        backend = batch.NativeBackend.__new__(batch.NativeBackend)
        backend.spec = {'commit': 'a' * 40}
        with patch.object(batch, 'read', return_value={'status': 'fail', 'package_commit': 'b' * 40}):
            with self.assertRaisesRegex(batch.SafetyError, 'different candidate'):
                backend.inspect({'name': 'tier1', 'report': '/unused', 'layer': 4})

    def test_passing_report_with_incomplete_cleanup_remains_failed(self):
        backend = batch.NativeBackend.__new__(batch.NativeBackend)
        backend.spec = {'commit': 'a' * 40}
        backend.identities = {'debian': {'sha256': 'c' * 64}}
        saved = {'status': 'pass', 'package_commit': 'a' * 40, 'package_sha256': 'c' * 64}
        binding = {'ubuntu': {'commit': 'a' * 40, 'sha256': 'c' * 64}}
        with patch.object(batch, 'read', return_value=saved), \
                patch.object(batch.release_status, 'e2e_details', return_value=('tier1', binding, False)):
            self.assertEqual(backend.inspect({'name': 'tier1', 'guest': 'ubuntu',
                                            'layer': 4, 'report': '/unused'})['status'], 'fail')

    def test_failed_owned_clone_cleanup_preserves_failure_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); runtime = root / 'runtime'; run_id = 'fresh-ubuntu'
            owner_path = runtime / run_id / 'ubuntu/owner.json'; owner_path.parent.mkdir(parents=True)
            evidence = root / 'results' / run_id / 'ubuntu/acceptance'; evidence.mkdir(parents=True)
            report = evidence / 'result.json'; report.write_text('{"status":"fail","error":"actual test failure"}')
            disk = owner_path.parent / 'disk.qcow2'; disk.write_bytes(b'owned clone fixture')
            owner = {'run_id': run_id, 'template': 'ubuntu', 'stage': 'prepared', 'uuid': 'owned-uuid',
                     'evidence': str(evidence.parent), 'paths': [str(disk)]}
            batch.save(owner_path, owner)
            calls = []
            def cleanup(record):
                calls.append('cleanup');disk.unlink();record['stage'] = 'removed';batch.save(owner_path, record)
            vm = SimpleNamespace(RUNTIME=runtime, unchanged=lambda x: calls.append('unchanged'),
                                 guarded=lambda x: calls.append('guarded'), cleanup=cleanup,
                                 virsh=lambda *args: SimpleNamespace(stdout='shut off'))
            backend = batch.NativeBackend.__new__(batch.NativeBackend)
            backend.vm=vm; backend.baseline={};backend.directory=root / 'batch';(backend.directory/'cleanup').mkdir(parents=True)
            before=report.read_bytes();backend.cleanup({'name':'failed-suite','run_id':'fresh','guest':'ubuntu'})
            self.assertFalse(disk.exists());self.assertEqual(report.read_bytes(),before)
            self.assertLess(calls.index('guarded'),calls.index('cleanup'))
            self.assertTrue(batch.read(backend.directory/'cleanup/failed-suite.json')['cleaned_after_suite'][0]['acceptance_evidence_unchanged'])


if __name__ == '__main__':
    unittest.main()
