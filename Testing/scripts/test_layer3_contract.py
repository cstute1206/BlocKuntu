"""Coverage and result gates must not mistake a partial run for acceptance."""
from copy import deepcopy
from pathlib import Path
import unittest

from layer3_contract import assess_observations, build_plan, load_contract, validate_contract


class ContractTests(unittest.TestCase):
    def setUp(self):
        self.registry, self.matrix = load_contract()

    def plan(self, **kwargs):
        return build_plan(self.registry, self.matrix, **kwargs)

    def results(self, plan):
        return [{**row, 'status': 'pass'} for row in plan['observations']]

    def test_specification_and_roadmap_inventory(self):
        self.assertEqual(len(self.registry['cases']), 77)
        self.assertEqual(sum(case['smoke'] for case in self.registry['cases']), 17)
        self.assertEqual(sum(case['deferred'] for case in self.registry['cases']), 1)
        positive = [v for v in self.matrix['variants'] if v['expected_supported']]
        self.assertEqual(len(positive), 12)
        self.assertEqual(sum(len(v['required_guests']) for v in positive), 30)
        full = self.plan(suite='full')
        self.assertEqual(full['case_count'], 76)
        self.assertEqual(full['deferred_cases'][0]['id'], 'VM-UPGRADE-001')

    def test_smoke_retains_exact_guest_and_browser_dimensions(self):
        for guest, browser in [('ubuntu', 'chrome-native'), ('fedora', 'chrome-native'),
                               ('cachyos', 'chromium-native')]:
            plan = self.plan(guests=[guest])
            self.assertEqual(plan['required_count'], 21)
            self.assertEqual(plan['implementation_counts'], {'automated': 21})
            self.assertEqual({r['browser_package'] for r in plan['observations']},
                             {None, 'firefox-native', browser})

    def test_smoke_observations_cannot_satisfy_full_gate(self):
        smoke = self.plan(guests=['ubuntu'])
        full = self.plan(suite='full', guests=['ubuntu'])
        result = assess_observations(full, self.results(smoke))
        self.assertEqual(result['observation_status'], 'incomplete')
        self.assertEqual(result['acceptance_status'], 'not_evaluated')

    def test_even_complete_observations_do_not_assert_vm_provenance(self):
        smoke = self.plan()
        result = assess_observations(smoke, self.results(smoke))
        self.assertEqual(result['observation_status'], 'pass')
        self.assertEqual(result['acceptance_status'], 'not_evaluated')

    def test_missing_browser_package_is_visible(self):
        plan = self.plan(guests=['ubuntu'])
        results = [r for r in self.results(plan) if r['browser_package'] != 'chrome-native']
        result = assess_observations(plan, results)
        self.assertEqual(result['observation_status'], 'incomplete')
        self.assertEqual(len(result['missing_required_observations']), 4)

    def test_duplicates_and_conflicting_results_fail(self):
        plan = self.plan(guests=['ubuntu'])
        for status in ('pass', 'fail', 'skip'):
            with self.subTest(status=status):
                results = self.results(plan)
                results.append({**results[0], 'status': status})
                self.assertEqual(assess_observations(plan, results)['observation_status'], 'fail')

    def test_setup_failure_unknown_id_and_wrong_dimension_cannot_be_hidden(self):
        plan = self.plan(guests=['ubuntu'])
        results = self.results(plan)
        for extra in ({'id': 'SETUP-GUEST', 'status': 'fail'},
                      {'observation_id': 'unknown', 'status': 'pass'}):
            with self.subTest(extra=extra):
                self.assertEqual(assess_observations(plan, results + [extra])['observation_status'], 'fail')
        results[0]['guest'] = 'fedora'
        self.assertEqual(assess_observations(plan, results)['observation_status'], 'fail')

    def test_skip_blocked_not_run_and_invalid_status_never_pass(self):
        plan = self.plan(guests=['ubuntu'])
        for status in ('skip', 'blocked', 'not_run', 'planned', 'nonsense'):
            with self.subTest(status=status):
                results = self.results(plan)
                results[0]['status'] = status
                self.assertNotEqual(assess_observations(plan, results)['observation_status'], 'pass')

    def test_unimplemented_scenarios_cannot_claim_a_pass(self):
        plan = self.plan(suite='core', guests=['ubuntu'], case_ids=['VM-AAL-011'])
        self.assertEqual({r['scenario'] for r in plan['observations']},
                         {'foreground', 'background', 'minimized'})
        self.assertEqual(assess_observations(plan, self.results(plan))['observation_status'], 'fail')

    def test_private_modes_are_family_specific(self):
        plan = self.plan(suite='browser-matrix', guests=['ubuntu'], case_ids=['VM-WAL-013'])
        firefox = {r['scenario'] for r in plan['observations'] if r['browser_package'] == 'firefox-native'}
        chrome = {r['scenario'] for r in plan['observations'] if r['browser_package'] == 'chrome-native'}
        self.assertEqual(firefox, {'regular', 'private'})
        self.assertIn('incognito-extension-permitted', chrome)
        self.assertNotIn('private', chrome)

    def test_confined_and_unsupported_cases_cannot_be_covered_by_native_browser(self):
        for cid, scope in [('VM-BR-007', 'confined'), ('VM-BR-008', 'negative')]:
            plan = self.plan(suite='browser-matrix', guests=['ubuntu'], case_ids=[cid])
            browsers = {r['browser_package'] for r in plan['observations']}
            self.assertNotIn('firefox-native', browsers)
            self.assertNotIn('chrome-native', browsers)
            self.assertIn('firefox-snap' if scope == 'confined' else 'chromium-flatpak', browsers)

    def test_selection_cannot_silently_ignore_invalid_requests(self):
        for kwargs in ({'case_ids': ['VM-NOPE-001']}, {'case_ids': ['VM-UPGRADE-001']},
                       {'case_ids': ['VM-WEB-008']}, {'browsers': ['chromium-flatpak']},
                       {'guests': []}, {'guests': ['debian']}, {'case_ids': []},
                       {'browsers': ['typo']}, {'guests': ['ubuntu', 'ubuntu']}):
            with self.subTest(kwargs=kwargs), self.assertRaises(ValueError):
                self.plan(**kwargs)
        plan = self.plan(case_ids=['VM-WEB-002'], guests=['ubuntu'])
        self.assertEqual(plan['scope'], 'selected')

    def test_extra_variants_and_strengthened_reboot_remain_planned(self):
        plan = self.plan(suite='full', guests=['ubuntu'])
        for row in plan['observations']:
            if row['browser_package'] == 'firefox-snap' or row['scenario'] == 'active-policy-reboot':
                self.assertEqual(row['implementation_status'], 'planned')

    def test_new_implementation_is_scoped_to_its_exact_binding(self):
        self.registry['implementation_bindings'].append({
            'id': 'VM-AAL-011', 'guest': 'ubuntu', 'browser_package': None,
            'scenario': 'foreground', 'status': 'automated', 'entrypoint': 'future-fixture.py'})
        plan = self.plan(suite='core', case_ids=['VM-AAL-011'])
        implemented = [r for r in plan['observations'] if r['implementation_status'] == 'automated']
        self.assertEqual(len(implemented), 1)
        self.assertEqual(implemented[0]['guest'], 'ubuntu')
        self.assertEqual(implemented[0]['scenario'], 'foreground')

    def test_duplicate_and_misattributed_bindings_are_rejected(self):
        source = (Path(__file__).parents[1] / 'layers/03-vm-acceptance.md').read_text()
        registry = deepcopy(self.registry)
        registry['implementation_bindings'].append(registry['implementation_bindings'][0])
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            validate_contract(registry, self.matrix, source)
        registry = deepcopy(self.registry)
        guest_only = next(row for row in registry['implementation_bindings']
                          if row['browser_package'] is None)
        guest_only['browser_package'] = 'firefox-native'
        with self.assertRaisesRegex(ValueError, 'Guest-only'):
            validate_contract(registry, self.matrix, source)

    def test_drift_and_duplicate_registry_entries_are_rejected(self):
        source = (Path(__file__).parents[1] / 'layers/03-vm-acceptance.md').read_text()
        registry = deepcopy(self.registry)
        registry['cases'][0]['expected'] = 'Different expected behavior'
        with self.assertRaisesRegex(ValueError, 'drift'):
            validate_contract(registry, self.matrix, source)
        registry = deepcopy(self.registry)
        registry['cases'].append(registry['cases'][0])
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            validate_contract(registry, self.matrix, source)
        registry = deepcopy(self.registry)
        registry['cases'].pop()
        with self.assertRaisesRegex(ValueError, 'differ'):
            validate_contract(registry, self.matrix, source)

    def test_invalid_matrix_and_additional_deferral_are_rejected(self):
        source = (Path(__file__).parents[1] / 'layers/03-vm-acceptance.md').read_text()
        matrix = deepcopy(self.matrix)
        matrix['core_browsers']['cachyos'] = ['firefox-native', 'chrome-native']
        with self.assertRaisesRegex(ValueError, 'Invalid core'):
            validate_contract(self.registry, matrix, source)
        registry = deepcopy(self.registry)
        registry['cases'][0]['deferred'] = True
        with self.assertRaisesRegex(ValueError, 'Only the agreed'):
            validate_contract(registry, self.matrix, source)

    def test_browser_cells_are_complete_and_unverified_cells_stay_explicit(self):
        self.assertEqual(len(self.matrix['cells']), 42)
        self.assertEqual(sum(cell['status'] == 'smoke-baseline'
                             for cell in self.matrix['cells'].values()), 6)
        source = (Path(__file__).parents[1] / 'layers/03-vm-acceptance.md').read_text()
        for mutation in ('missing-cell', 'planned-with-path', 'baseline-without-source',
                         'baseline-outside-core', 'unsafe-profile'):
            matrix = deepcopy(self.matrix)
            if mutation == 'missing-cell':
                matrix['cells'].pop('ubuntu/firefox-native')
            elif mutation == 'planned-with-path':
                matrix['cells']['ubuntu/firefox-snap']['executable'] = '/usr/bin/firefox'
            elif mutation == 'baseline-without-source':
                matrix['cells']['ubuntu/firefox-native']['source'] = None
            elif mutation == 'baseline-outside-core':
                matrix['cells']['ubuntu/firefox-snap']['status'] = 'smoke-baseline'
                matrix['cells']['ubuntu/firefox-snap']['pending_reason'] = None
            else:
                matrix['cells']['ubuntu/firefox-native']['profile_root'] = '/home/akhi/.mozilla'
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                validate_contract(self.registry, matrix, source)

    def test_implemented_scenarios_require_bounded_budgets(self):
        source = (Path(__file__).parents[1] / 'layers/03-vm-acceptance.md').read_text()
        for budget in (None, 0, True, 1801):
            registry = deepcopy(self.registry)
            scenario = registry['cases'][0]['scenarios'][0]
            scenario['budget_seconds'] = budget
            with self.subTest(budget=budget), self.assertRaises(ValueError):
                validate_contract(registry, self.matrix, source)


if __name__ == '__main__':
    unittest.main()
