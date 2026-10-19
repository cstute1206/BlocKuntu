"""Regress the terminal-checkpoint/service-exit race without a VM."""
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location('tier1_runner', Path(__file__).with_name('tier1-edit-acceptance.py'))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class CompletionRaceTests(unittest.TestCase):
    def test_credential_completion_reports_length_without_the_input_value(self):
        action = {'id': 'a' * 32, 'action': 'type_credential', 'slot': 'invalid'}
        replies = []
        def ssh(record, command, **kwargs):
            if 'host-action-done.json' in command:
                replies.append(json.loads(kwargs['input']))
            if 'cat Testing/layer3/host-action.json' in command:
                value = action
            elif 'protected-result.json' in command:
                value = {'status': 'pass', 'cases': []}
            else:
                return SimpleNamespace(stdout='', returncode=0)
            return SimpleNamespace(stdout=json.dumps(value), returncode=0)
        vm = SimpleNamespace(ssh=ssh, save=Mock(), guarded=Mock(), virsh=Mock())
        with patch.object(runner, 'vm', vm), patch.object(runner.suite, 'upload'), \
                patch.object(runner.time, 'sleep'):
            runner.exercise({'template': 'fedora', 'uuid': 'fixture'}, Path('/unused-test-destination'))
        self.assertEqual(replies, [{'id': action['id'],
            'typed_characters': len('BLOCKUNTU-TIER1-EDIT-INVALID')}])
        self.assertNotIn('BLOCKUNTU-', json.dumps(replies))

    def exercise_after_exit(self, terminal):
        reads = []
        def ssh(record, command, **kwargs):
            if 'cat Testing/layer3/protected-result.json' in command:
                reads.append(command)
                value = {'status': 'running', 'cases': []} if len(reads) == 1 else terminal
                return SimpleNamespace(stdout=json.dumps(value), returncode=0)
            if 'is-active' in command:
                return SimpleNamespace(stdout='inactive\n', returncode=3)
            return SimpleNamespace(stdout='', returncode=0)
        vm = SimpleNamespace(ssh=ssh, save=lambda *args: None)
        with patch.object(runner, 'vm', vm), patch.object(runner.suite, 'upload'):
            return runner.exercise({'template': 'fedora'}, Path('/unused-test-destination'))

    def test_terminal_checkpoint_written_between_read_and_service_exit(self):
        for status in ('pass', 'fail'):
            with self.subTest(status=status):
                terminal = {'status': status, 'cases': []}
                self.assertEqual(self.exercise_after_exit(terminal), terminal)

    def test_exit_with_only_partial_checkpoint_is_still_a_failure(self):
        with self.assertRaisesRegex(RuntimeError, 'before a terminal checkpoint'):
            self.exercise_after_exit({'status': 'running', 'cases': []})


if __name__ == '__main__':
    unittest.main()
