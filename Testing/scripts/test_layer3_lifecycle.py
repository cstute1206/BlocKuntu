"""Regression boundaries for the small Ubuntu lifecycle selection."""
import copy
import datetime as dt
import importlib.util
from pathlib import Path
import sys
import unittest

from layer3_contract import load_contract
from layer3_runtime import execution_plans, p32_website_plan
from layer3_group_runner import lifecycle_assertion_valid, isolated_assertion_valid

FIXTURES = Path(__file__).resolve().parents[1] / 'fixtures/layer3'
sys.path.insert(0, str(FIXTURES))
spec = importlib.util.spec_from_file_location('lifecycle_fixture', FIXTURES / 'lifecycle.py')
lifecycle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lifecycle)


class LifecycleTests(unittest.TestCase):
    def test_explicit_ubuntu_selection_only(self):
        registry, matrix = load_contract()
        for cases in (['VM-ACT-001'], ['VM-ACT-002'], ['VM-WEB-008'],
                      ['VM-ACT-001', 'VM-ACT-002', 'VM-WEB-008']):
            _, plan = execution_plans(registry, matrix, guest='ubuntu', suite='core', case_ids=cases)
            self.assertTrue(p32_website_plan(plan))
            self.assertEqual(len(plan['observations']), 2 * len(cases))
            for guest in ('fedora', 'cachyos'):
                with self.assertRaises(ValueError):
                    execution_plans(registry, matrix, guest=guest, suite='core', case_ids=cases)

    def test_unlock_requires_one_browser_and_application_stays_guest_only(self):
        registry, matrix = load_contract()
        with self.assertRaises(ValueError):
            execution_plans(registry, matrix, guest='ubuntu', suite='core', case_ids=['VM-WAL-012'])
        for browser in ('firefox-native', 'chrome-native'):
            _, plan = execution_plans(registry, matrix, guest='ubuntu', suite='core',
                                     case_ids=['VM-WAL-012'], browsers=[browser])
            self.assertEqual(len(plan['observations']), 1)
            self.assertTrue(p32_website_plan(plan))
        _, plan = execution_plans(registry, matrix, guest='ubuntu', suite='core',
                                 case_ids=['VM-AAL-010'])
        self.assertEqual(len(plan['observations']), 1)
        self.assertIsNone(plan['observations'][0]['browser_package'])

    def test_window_leaves_baseline_and_crosses_midnight(self):
        for second in (0, 29, 59):
            now = dt.datetime(2026, 10, 1, 23, 59, second, tzinfo=dt.timezone.utc)
            start, end = lifecycle.schedule_window(now)
            self.assertGreaterEqual((start - now).total_seconds(), 70)
            self.assertLessEqual((start - now).total_seconds(), 130)
            self.assertEqual((end - start).total_seconds(), 180)
            self.assertEqual(start.day, 2)

    def test_counter_or_unrelated_block_cannot_prove_exhaustion(self):
        evidence = {'rule_id': 'site', 'remaining_after_positive_usage': 45,
                    'remaining_after_exhaustion': 0, 'exhaustion_seconds': 65,
                    'blocked_decision': {'decision': 'block', 'reason': {
                        'rule_id': 'site', 'controlled_reason': 'allowance_exhausted'}}}
        self.assertTrue(lifecycle_assertion_valid('VM-WEB-008', evidence))
        for key, value in [('remaining_after_positive_usage', 60),
                           ('remaining_after_exhaustion', 10), ('exhaustion_seconds', 121)]:
            self.assertFalse(lifecycle_assertion_valid('VM-WEB-008', {**evidence, key: value}))
        changed = copy.deepcopy(evidence)
        changed['blocked_decision']['reason']['rule_id'] = 'other'
        self.assertFalse(lifecycle_assertion_valid('VM-WEB-008', changed))

    def test_app_lifecycle_cannot_pass_without_expiry_and_zero_remaining(self):
        evidence = {'enforcement_active': True, 'rule_id': 'app',
                    'allowed_only_uncharged': True, 'positive_usage': True,
                    'unlocked_process_survived': True, 'reblocked_after_expiry': True,
                    'grant_seconds': 120, 'remaining_after_exhaustion': 0,
                    'remaining_after_positive_usage': 50, 'exhaustion_seconds': 62,
                    'reblock_seconds': 9, 'grant': {'rule_id': 'app'},
                    'allowed_process': {'pid': 100, 'start_ticks': '40', 'exe': '/tmp/bk-test-app-a'},
                    'rejected_process': {'pid': 101, 'start_ticks': '41', 'exe': '/tmp/bk-test-app-b'},
                    'unlocked_process': {'pid': 102, 'start_ticks': '400', 'exe': '/tmp/bk-test-app-b'}}
        self.assertFalse(isolated_assertion_valid('VM-AAL-010', {**evidence, 'unlocked_process': {}}))
        self.assertFalse(isolated_assertion_valid('VM-AAL-010', {
            **evidence, 'unlocked_process': evidence['rejected_process']}))
        self.assertTrue(isolated_assertion_valid('VM-AAL-010', evidence))
        for key, value in [('reblocked_after_expiry', False), ('remaining_after_exhaustion', 60),
                           ('grant_seconds', 60), ('reblock_seconds', 31)]:
            self.assertFalse(isolated_assertion_valid('VM-AAL-010', {**evidence, key: value}))

    def test_process_recovery_needs_distinct_observed_identity(self):
        evidence = {'rule_id': 'site', 'activation': 'schedule', 'process_exit_seconds': 9,
                    'before_process': {'pid': 100, 'start_ticks': '40', 'exe': '/tmp/bk-test-app-a'},
                    'after_process': {'pid': 101, 'start_ticks': '400', 'exe': '/tmp/bk-test-app-a'},
                    'blocked_decision': {'decision': 'block', 'reason': {'rule_id': 'site'}}}
        self.assertTrue(lifecycle_assertion_valid('VM-ACT-001', evidence))
        evidence['after_process'] = evidence['before_process']
        self.assertFalse(lifecycle_assertion_valid('VM-ACT-001', evidence))


if __name__ == '__main__':
    unittest.main()
