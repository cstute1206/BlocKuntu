"""Saved group audits must retain failures and require latest clean evidence."""
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch

from layer3_contract import ROOT
from layer3_group_audit import audit_summaries


class GroupAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for directory in ('layer3', 'config', 'layers'):
            (self.root / directory).symlink_to(ROOT / directory, target_is_directory=True)
        self.commit = 'a' * 40
        self.first = self.summary('fail', [
            self.make_run('ubuntu', 'one-ubuntu', 'pass', 0),
            self.make_run('fedora', 'one-fedora', 'pass', 2),
            self.make_run('cachyos', 'one-cachyos', 'fail', 4)])
        self.retry = self.summary('pass', [self.make_run('cachyos', 'two-cachyos', 'pass', 6)])
        self.paths = [self.root / 'first.json', self.root / 'retry.json']
        self.write()
        for run in [*self.first['runs'], *self.retry['runs']]:
            owner = self.root / 'runtime/vm-phase0' / run['run_id'] / run['guest'] / 'owner.json'
            owner.parent.mkdir(parents=True)
            owner.write_text(json.dumps({'run_id': run['run_id'], 'template': run['guest'],
                'stage': 'removed', 'paths': [str(self.root / (run['run_id'] + '-disk'))],
                'uuid': run['run_id'] + '-uuid'}))

    def make_run(self, guest, run_id, status, minute):
        return {'guest': guest, 'group_id': f'{guest}:smoke-baseline', 'run_id': run_id,
                'status': status, 'exit_code': 0 if status == 'pass' else 1,
                'started_at': f'2026-09-28T10:{minute:02d}:00+00:00',
                'finished_at': f'2026-09-28T10:{minute + 1:02d}:00+00:00'}

    def summary(self, status, runs):
        return {'kind': 'layer3-group-run', 'suite': 'smoke', 'scope': 'smoke-only',
                'status': status, 'package_commit': self.commit,
                'workflow': {'headSha': self.commit},
                'packages': {guest: {'commit': self.commit} for guest in
                             ('ubuntu', 'fedora', 'cachyos')}, 'runs': runs}

    def write(self):
        self.paths[0].write_text(json.dumps(self.first))
        self.paths[1].write_text(json.dumps(self.retry))

    def evidence(self, item, expected, plan, **kwargs):
        return {'valid': True, 'errors': [], 'required_passes': 21,
                'clone_uuid': item['run_id'] + '-uuid',
                'machine_id': item['run_id'] + '-machine',
                'host_fingerprint': item['run_id'] + '-key',
                'manifest': {'scripts/vm-acceptance.py': 'b' * 64}}

    def test_latest_pass_retains_failed_attempt_without_claiming_live_check(self):
        with patch('layer3_group_audit.verify_bundle', side_effect=self.evidence):
            audit = audit_summaries(self.paths, root=self.root)
        self.assertEqual(audit['status'], 'incomplete')
        self.assertEqual(audit['required_observations_passed'], 63)
        self.assertEqual([row['run_id'] for row in audit['earlier_attempts']], ['one-cachyos'])
        self.assertEqual(audit['live_inventory_check'], 'not_performed')

    def test_latest_failure_cannot_be_hidden_by_earlier_pass(self):
        self.retry['runs'][0]['status'] = 'fail'
        self.retry['status'] = 'fail'
        self.write()
        with self.assertRaisesRegex(ValueError, 'latest attempted group did not pass'):
            audit_summaries(self.paths, root=self.root)

    def test_overlapping_groups_and_owned_storage_fail(self):
        self.first['runs'][1]['started_at'] = '2026-09-28T10:00:30+00:00'
        self.write()
        with patch('layer3_group_audit.verify_bundle', side_effect=self.evidence), \
             self.assertRaisesRegex(ValueError, 'overlapped'):
            audit_summaries(self.paths, root=self.root)
        self.first['runs'][1]['started_at'] = '2026-09-28T10:02:00+00:00'
        self.write()
        (self.root / 'one-cachyos-disk').write_text('still owned')
        with patch('layer3_group_audit.verify_bundle', side_effect=self.evidence), \
             self.assertRaisesRegex(ValueError, 'copied storage'):
            audit_summaries(self.paths, root=self.root)

    def test_live_inventory_required_for_pass(self):
        templates = {guest: {'domain': guest.title()} for guest in ('ubuntu', 'fedora', 'cachyos')}
        def virsh(command, *args):
            if command == 'list' and args[-1] == '--name':
                return types.SimpleNamespace(stdout='Ubuntu\nFedora\nCachyos\n')
            if command == 'list' and args[-1] == '--uuid':
                return types.SimpleNamespace(stdout='base-1\nbase-2\nbase-3\n')
            return types.SimpleNamespace(stdout='shut off\n')
        vm = types.SimpleNamespace(CONFIG={'templates': templates},
                                   base_records=lambda: {}, unchanged=lambda owner: None,
                                   virsh=virsh)
        with patch('layer3_group_audit.verify_bundle', side_effect=self.evidence), \
             patch('layer3_group_audit.phase0_module', return_value=vm):
            audit = audit_summaries(self.paths, root=self.root, live=True)
        self.assertEqual(audit['status'], 'pass')
        self.assertEqual(audit['live_inventory_check'], 'pass')


if __name__ == '__main__':
    unittest.main()
