"""Fresh-clone planning must never collapse independent unimplemented scenarios."""
import unittest

from layer3_contract import build_plan, load_contract
from layer3_groups import build_execution_groups


class GroupTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.registry, cls.matrix = load_contract()

    def plan(self, **kwargs):
        return build_plan(self.registry, self.matrix, **kwargs)

    def test_whole_smoke_has_one_known_group_per_guest(self):
        groups = build_execution_groups(self.plan())
        self.assertEqual(groups['group_count'], 3)
        self.assertEqual(groups['runnable_group_count'], 3)
        self.assertEqual(groups['required_observation_count'], 63)
        self.assertTrue(all(len(g['observation_ids']) == 21 and
                            g['budget_seconds'] == 2520 and g['clone_policy'] == 'fresh-copy'
                            for g in groups['groups']))

    def test_full_and_selected_cases_remain_independent_and_not_runnable(self):
        for kwargs in ({'suite': 'full', 'guests': ['ubuntu']},
                       {'suite': 'smoke', 'guests': ['fedora'], 'case_ids': ['VM-WEB-002']},
                       {'suite': 'browser-matrix', 'guests': ['cachyos']}):
            with self.subTest(kwargs=kwargs):
                plan = self.plan(**kwargs)
                groups = build_execution_groups(plan)
                self.assertEqual(groups['group_count'], plan['required_count'])
                self.assertEqual(groups['runnable_group_count'], 0)
                self.assertTrue(all(len(g['observation_ids']) == 1 and
                                    g['clone_policy'] == 'fresh-copy' for g in groups['groups']))

    def test_implemented_selected_groups_have_planning_budgets_but_no_executor(self):
        groups = build_execution_groups(self.plan(suite='full', guests=['ubuntu']))
        bounded = [group for group in groups['groups'] if group['budget_seconds'] is not None]
        self.assertEqual(len(bounded), 71)
        self.assertTrue(all(not group['runnable'] and group['executor'] is None and
                            group['budget_seconds'] > 0 for group in bounded))

    def test_only_isolated_ubuntu_tier1_allowlist_is_runnable(self):
        ubuntu = build_execution_groups(self.plan(suite='core', guests=['ubuntu'],
                                                   case_ids=['VM-WAL-001']))
        self.assertEqual(ubuntu['runnable_group_count'], 1)
        self.assertEqual(ubuntu['groups'][0]['executor'], 'vm-acceptance.py')
        fedora = build_execution_groups(self.plan(suite='core', guests=['fedora'],
                                                   case_ids=['VM-WAL-001']))
        self.assertEqual(fedora['runnable_group_count'], 0)

    def test_selected_ubuntu_cases_have_independent_fresh_clones(self):
        groups = build_execution_groups(self.plan(suite='core', guests=['ubuntu'],
            case_ids=['VM-AAL-001', 'VM-WAL-005']))
        self.assertEqual(groups['runnable_group_count'], 2)
        self.assertEqual(groups['required_observation_count'], 2)
        self.assertTrue(all(group['clone_policy'] == 'fresh-copy' and
                            len(group['observation_ids']) == 1 for group in groups['groups']))

    def test_p32_ubuntu_website_matchers_share_one_bounded_clone(self):
        selected = ['VM-WEB-003', 'VM-WEB-004', 'VM-WEB-005', 'VM-WEB-006', 'VM-WEB-007']
        groups = build_execution_groups(self.plan(suite='core', guests=['ubuntu'], case_ids=selected))
        self.assertEqual(groups['group_count'], 1)
        self.assertEqual(groups['runnable_group_count'], 1)
        self.assertEqual(groups['required_observation_count'], 10)
        self.assertEqual(groups['groups'][0]['executor'], 'vm-acceptance.py')
        self.assertEqual(groups['groups'][0]['clone_policy'], 'fresh-copy')
        self.assertGreater(groups['groups'][0]['budget_seconds'], 2520)

    def test_p32_ubuntu_domain_uses_separate_bounded_clone(self):
        groups = build_execution_groups(self.plan(suite='core', guests=['ubuntu'],
                                                  case_ids=['VM-WEB-001']))
        self.assertEqual(groups['group_count'], 1)
        self.assertEqual(groups['required_observation_count'], 2)
        self.assertEqual(groups['groups'][0]['group_id'], 'ubuntu:p32-native-domain')
        self.assertTrue(groups['groups'][0]['runnable'])

    def test_changed_whole_smoke_contract_fails_closed(self):
        plan = self.plan(guests=['ubuntu'])
        plan['observations'].pop()
        with self.assertRaisesRegex(ValueError, 'contract changed'):
            build_execution_groups(plan)


if __name__ == '__main__':
    unittest.main()
