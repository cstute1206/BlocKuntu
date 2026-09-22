"""Validate artifact trust and result boundaries without booting a guest."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('acceptance', Path(__file__).with_name('vm-acceptance.py'))
suite = importlib.util.module_from_spec(spec)
spec.loader.exec_module(suite)


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'artifacts').mkdir()
        self.package = self.root / 'artifacts/test.deb'
        self.package.write_bytes(b'exact CI payload')
        self.report = {'passed': True, 'build_provenance_verified': True, 'component': 'deb',
                       'commit': 'a' * 40, 'artifacts': [{'filename': 'test.deb', 'sha256': hashlib.sha256(self.package.read_bytes()).hexdigest()}]}
        self.path = self.root / 'result.json'

    def check_report(self):
        self.path.write_text(json.dumps(self.report))
        return suite.artifact(self.path)

    def test_accepts_exact_verified_package(self):
        self.assertEqual(self.check_report()[0], self.package)

    def test_rejects_mutated_artifact(self):
        self.package.write_bytes(b'replaced')
        with self.assertRaisesRegex(ValueError, 'SHA-256'):
            self.check_report()

    def test_rejects_unverified_source(self):
        self.report['build_provenance_verified'] = False
        with self.assertRaisesRegex(ValueError, 'provenance'):
            self.check_report()

    def test_rejects_failed_package_inspection(self):
        self.report['passed'] = False
        with self.assertRaises(ValueError):
            self.check_report()

    def test_rejects_path_escape(self):
        self.report['artifacts'][0]['filename'] = '../test.deb'
        with self.assertRaisesRegex(ValueError, 'basename'):
            self.check_report()

    def test_rejects_ambiguous_packages(self):
        self.report['artifacts'].append(self.report['artifacts'][0])
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            self.check_report()

    def test_rejects_symlink_package(self):
        target = self.root / 'other'
        self.package.rename(target)
        self.package.symlink_to(target)
        with self.assertRaises(ValueError):
            self.check_report()


class BrowserEvidenceTests(unittest.TestCase):
    def test_disabled_install_confirmation_is_not_activated(self):
        fake = types.SimpleNamespace(STATE_ENABLED=1, STATE_SENSITIVE=2,
            Atspi=types.SimpleNamespace(set_timeout=lambda *args: None))
        spec = importlib.util.spec_from_file_location('acceptance_accessibility',
            Path(__file__).parents[1] / 'fixtures/layer3/accessibility.py')
        module = importlib.util.module_from_spec(spec)
        with patch.dict('sys.modules', pyatspi=fake):
            spec.loader.exec_module(module)
        # The button exists, but its security delay has not expired. Do not
        # interpret the accessibility action's potential no-op as a click.
        node = types.SimpleNamespace(getState=lambda: types.SimpleNamespace(contains=lambda state: False))
        with patch.object(module, 'find', return_value=node), \
             patch.object(module.time, 'monotonic', side_effect=[0, 16]):
            with self.assertRaisesRegex(TimeoutError, 'remained disabled'):
                module.click('Add', app='Firefox')
        activated = []
        sensitive = types.SimpleNamespace(
            getState=lambda: types.SimpleNamespace(contains=lambda state: state == fake.STATE_SENSITIVE),
            queryAction=lambda: types.SimpleNamespace(doAction=lambda index: activated.append(index) or True))
        with patch.object(module, 'find', return_value=sensitive):
            module.click('Select', app='org.gnome.Nautilus')
        self.assertEqual(activated, [0], 'GTK4 controls may be sensitive without ENABLED')
        with patch.object(module, 'find', return_value=sensitive), \
             patch.object(module.time, 'monotonic', side_effect=[0, 16]):
            with self.assertRaises(TimeoutError):
                module.click('Add', app='Firefox')
        self.assertEqual(activated, [0], 'Browser permission delays also require ENABLED')

    def test_install_confirmation_requires_prompt_dismissal(self):
        accessibility = types.ModuleType('accessibility')
        for name in ('click', 'dump', 'find', 'visible_document_text', 'walk', 'apps', 'app_name'):
            setattr(accessibility, name, lambda *args, **kwargs: None)
        guest = types.ModuleType('guest')
        guest.WORK = Path('/unused')
        for name in ('host_action', 'rpc', 'run', 'wait'):
            setattr(guest, name, lambda *args, **kwargs: None)
        spec = importlib.util.spec_from_file_location('acceptance_browsers',
            Path(__file__).parents[1] / 'fixtures/layer3/browsers.py')
        module = importlib.util.module_from_spec(spec)
        with patch.dict('sys.modules', pyatspi=types.ModuleType('pyatspi'),
                        accessibility=accessibility, guest=guest):
            spec.loader.exec_module(module)
        with patch.object(module, 'click') as click, \
             patch.object(module, 'find', side_effect=[object(), TimeoutError()]), \
             patch.object(module.time, 'sleep'):
            module.confirm_install('Add', 'Firefox')
            self.assertEqual(click.call_count, 2, 'A no-op action must not start heartbeat timing')
        with patch.object(module, 'click'), patch.object(module, 'find', return_value=object()), \
             patch.object(module.time, 'monotonic', side_effect=[0, 16]):
            with self.assertRaisesRegex(TimeoutError, 'prompt did not close'):
                module.confirm_install('Add', 'Firefox')

    def test_background_tab_and_toolbar_cannot_satisfy_page_assertion(self):
        class Node:
            def __init__(self, name, role, showing=True, children=()):
                self.name, self.role, self.showing, self.children = name, role, showing, children

            def __iter__(self):
                return iter(self.children)

            def getRoleName(self):
                return self.role

            def getState(self):
                return types.SimpleNamespace(contains=lambda state: self.showing)

            def queryText(self):
                return types.SimpleNamespace(characterCount=len(self.name), getText=lambda a, b: self.name)

        browser = Node('Firefox', 'application', children=[
            Node('Blocked by BlocKuntu', 'page tab'),
            Node('Hidden allowed page', 'document web', showing=False),
            Node('Current page', 'document web', children=[Node('Current body', 'paragraph')]),
        ])
        fake = types.SimpleNamespace(STATE_SHOWING=1,
            Atspi=types.SimpleNamespace(set_timeout=lambda *args: None),
            Registry=types.SimpleNamespace(getDesktop=lambda index: [browser]))
        spec = importlib.util.spec_from_file_location('acceptance_accessibility',
            Path(__file__).parents[1] / 'fixtures/layer3/accessibility.py')
        module = importlib.util.module_from_spec(spec)
        with patch.dict('sys.modules', pyatspi=fake):
            spec.loader.exec_module(module)
            observed = module.visible_document_text('Firefox')
        self.assertIn('Current page', observed)
        self.assertIn('Current body', observed)
        self.assertNotIn('Hidden allowed page', observed)
        self.assertNotIn('Blocked by BlocKuntu', observed)


class KeyInputTests(unittest.TestCase):
    def test_accepts_only_fixture_and_policy_urls(self):
        for url in ('about:policies', 'chrome://policy', 'http://web.blockuntu.test:18080/exact/blocked'):
            keys = suite.url_keys(url)
            self.assertIn(('KEY_LEFTSHIFT', 'KEY_SEMICOLON'), keys)
        for url in ('https://example.com', 'file:///etc/passwd', 'javascript:alert(1)',
                    'http://web.blockuntu.test:18080/free\n', 'http://web.blockuntu.test:18080/;exit'):
            with self.assertRaises(ValueError):
                suite.url_keys(url)


class ResultTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('acceptance_guest',
            Path(__file__).parents[1] / 'fixtures/layer3/guest.py')
        self.guest = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.guest)

    def passing_cases(self):
        empty = {'cases': []}
        self.guest.finalize_result(empty)
        return [{**row, 'status': 'pass'} for row in empty['cases']]

    def test_all_required_observations_pass(self):
        result = {'cases': self.passing_cases()}
        self.guest.finalize_result(result)
        self.assertEqual(result['status'], 'pass')
        self.assertEqual(result['missing_required_cases'], [])

    def test_firefox_cannot_cover_missing_chrome(self):
        result = {'cases': [row for row in self.passing_cases() if row.get('browser') != 'chrome']}
        self.guest.finalize_result(result)
        self.assertEqual(result['status'], 'incomplete')
        self.assertIn('VM-BR-001:chrome', result['missing_required_cases'])
        self.assertEqual(sum(row['status'] == 'blocked' for row in result['cases']), 4)

    def test_extra_setup_failure_cannot_be_hidden_by_passing_cases(self):
        result = {'cases': self.passing_cases() + [{'id': 'SETUP-GUEST', 'status': 'fail'}]}
        self.guest.finalize_result(result)
        self.assertEqual(result['status'], 'fail')


if __name__ == '__main__':
    unittest.main()
