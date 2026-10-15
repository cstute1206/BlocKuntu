"""Runtime smoke adapter and preflight must fail closed around partial coverage."""
import unittest

from layer3_contract import load_contract
from layer3_runtime import (evaluate_smoke, execution_plans, process_budget_seconds,
                            p32_website_budget_seconds, p32_website_plan,
                            require_stage_passed, smoke_budget_seconds)


class RuntimeGateTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.registry, cls.matrix = load_contract()

    def plans(self, **kwargs):
        return execution_plans(self.registry, self.matrix, guest='ubuntu', **kwargs)

    def cases(self, plan):
        return [{'id': row['id'], 'browser': row['browser_package'].split('-')[0]
                 if row['browser_package'] else None, 'status': 'pass'}
                for row in plan['observations']]

    def test_smoke_plan_and_pass_have_exact_dimensions(self):
        smoke, selected = self.plans()
        self.assertEqual(smoke, selected)
        self.assertEqual(smoke['required_count'], 21)
        observations, coverage = evaluate_smoke(smoke, self.cases(smoke))
        self.assertEqual(coverage['observation_status'], 'pass')
        self.assertEqual({row['guest'] for row in observations}, {'ubuntu'})
        self.assertEqual({row['scenario'] for row in observations}, {'baseline'})
        self.assertEqual({row['browser_package'] for row in observations},
                         {None, 'firefox-native', 'chrome-native'})

    def test_other_guest_browser_cannot_fill_required_cell(self):
        smoke, _ = self.plans()
        cases = self.cases(smoke)
        browser = next(row for row in cases if row['browser'] == 'chrome')
        cid = browser['id']
        browser['browser'] = 'chromium'
        _, coverage = evaluate_smoke(smoke, cases)
        self.assertEqual(coverage['observation_status'], 'fail')
        self.assertIn(f'ubuntu:{cid}:chrome-native:baseline',
                      coverage['missing_required_observations'])

    def test_conflicting_explicit_dimensions_are_not_overwritten(self):
        smoke, _ = self.plans()
        for field, value in [('guest', 'fedora'), ('browser_package', 'firefox-snap'),
                             ('scenario', 'other'), ('observation_id', 'invented')]:
            cases = self.cases(smoke)
            cases[0][field] = value
            with self.subTest(field=field):
                _, coverage = evaluate_smoke(smoke, cases)
                self.assertEqual(coverage['observation_status'], 'fail')

    def test_setup_failure_and_duplicate_cannot_hide_behind_passing_cases(self):
        smoke, _ = self.plans()
        cases = self.cases(smoke)
        for extra in ({'id': 'SETUP-GUI', 'status': 'fail'}, dict(cases[0])):
            with self.subTest(extra=extra):
                _, coverage = evaluate_smoke(smoke, cases + [extra])
                self.assertEqual(coverage['observation_status'], 'fail')

    def test_skipped_deferred_upgrade_does_not_count_as_pass(self):
        smoke, _ = self.plans()
        cases = self.cases(smoke)
        cases.append({'id': 'VM-UPGRADE-001', 'status': 'skip', 'reason': 'User deferral'})
        _, coverage = evaluate_smoke(smoke, cases)
        self.assertEqual(coverage['observation_status'], 'pass')
        cases[-1]['status'] = 'pass'
        _, coverage = evaluate_smoke(smoke, cases)
        self.assertEqual(coverage['observation_status'], 'fail')

    def test_unimplemented_suites_rejected_before_vm(self):
        for suite in ('core', 'browser-matrix', 'full'):
            with self.subTest(suite=suite), self.assertRaisesRegex(ValueError,
                                                                  'without an executable smoke binding'):
                self.plans(suite=suite)

    def test_selected_smoke_case_is_diagnostic_projection(self):
        smoke, selected = self.plans(suite='core', case_ids=['VM-WEB-002'])
        self.assertEqual(smoke['required_count'], 21)
        self.assertEqual(selected['scope'], 'selected')
        self.assertEqual(selected['required_count'], 2)
        self.assertEqual({row['id'] for row in selected['observations']}, {'VM-WEB-002'})

    def test_p32_ubuntu_native_matcher_group_has_exact_cells(self):
        cases = ['VM-WEB-003', 'VM-WEB-004', 'VM-WEB-005', 'VM-WEB-006', 'VM-WEB-007']
        smoke, selected = self.plans(suite='core', case_ids=cases)
        self.assertTrue(p32_website_plan(selected))
        self.assertEqual(selected['required_count'], 10)
        self.assertEqual({row['browser_package'] for row in selected['observations']},
                         {'firefox-native', 'chrome-native'})
        self.assertGreater(p32_website_budget_seconds(smoke, selected), smoke_budget_seconds(smoke))
        with self.assertRaises(ValueError):
            self.plans(suite='core', case_ids=cases[:-1])

    def test_p32_ubuntu_domain_group_has_two_native_cells(self):
        smoke, selected = self.plans(suite='core', case_ids=['VM-WEB-001'])
        self.assertTrue(p32_website_plan(selected))
        self.assertEqual(selected['required_count'], 2)
        self.assertGreater(p32_website_budget_seconds(smoke, selected), smoke_budget_seconds(smoke))

    def test_budgets_are_derived_from_bound_smoke_scenarios(self):
        smoke, _ = self.plans()
        self.assertEqual(smoke_budget_seconds(smoke), 2520)
        self.assertEqual(process_budget_seconds(smoke), 470)
        target = next(row for row in smoke['observations'] if row['id'] == 'VM-AAL-004')
        target['budget_seconds'] += 30
        self.assertEqual(smoke_budget_seconds(smoke), 2550)
        self.assertEqual(process_budget_seconds(smoke), 500)
        target['budget_seconds'] = None
        with self.assertRaises(ValueError):
            smoke_budget_seconds(smoke)
        with self.assertRaises(ValueError):
            process_budget_seconds(smoke)

    def test_dependency_stages_reject_missing_failed_or_duplicate_cases(self):
        smoke, _ = self.plans()
        cases = self.cases(smoke)
        for stage in ('services', 'gui', 'browsers', 'processes'):
            require_stage_passed(smoke, cases, stage)
            example = next(row for row in cases if row['id'] in {
                'services': ('VM-INSTALL-002',), 'gui': ('VM-DATA-001',),
                'browsers': ('VM-BR-001',), 'processes': ('VM-APP-001',)}[stage])
            for broken in ([row for row in cases if row is not example],
                           [{**row, 'status': 'fail'} if row is example else row for row in cases],
                           cases + [dict(example)]):
                with self.subTest(stage=stage, count=len(broken)), \
                     self.assertRaisesRegex(RuntimeError, 'did not pass'):
                    require_stage_passed(smoke, broken, stage)

    def test_unimplemented_or_deferred_case_rejected(self):
        for cid in ('VM-AAL-011', 'VM-UPGRADE-001'):
            with self.subTest(case=cid), self.assertRaises(ValueError):
                self.plans(suite='full', case_ids=[cid])


if __name__ == '__main__':
    unittest.main()
