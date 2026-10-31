"""Smoke fixtures must not execute cases after their setup dependency fails."""
import importlib.util
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch


def load_fixture(name, work):
    accessibility = types.ModuleType('accessibility')
    for function in ('app_name', 'apps', 'click', 'dump', 'find', 'texts', 'walk',
                     'visible_document_text', 'refresh_desktop_cache'):
        setattr(accessibility, function, lambda *args, **kwargs: {})
    guest = types.ModuleType('guest')
    guest.WORK = work
    for function in ('host_action', 'run', 'wait'):
        setattr(guest, function, lambda *args, **kwargs: None)
    guest.rpc = lambda *args, **kwargs: {'rules': []}
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1] /
                                                  f'fixtures/layer3/{name}.py')
    module = importlib.util.module_from_spec(spec)
    with patch.dict('sys.modules', pyatspi=types.ModuleType('pyatspi'),
                    accessibility=accessibility, guest=guest):
        spec.loader.exec_module(module)
    return module


class DependencyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.work = Path(self.temp.name)

    def test_failed_welcome_stops_policy_and_browser_prerequisites(self):
        gui = load_fixture('gui', self.work)
        calls = []
        def case(cid, action):
            calls.append(cid)
            return False
        with self.assertRaisesRegex(RuntimeError, 'Welcome setup failed'):
            gui.run_cases(case, 'test-run')
        self.assertEqual(calls, ['VM-INSTALL-004'])

    def test_failed_import_stops_dependent_data_cases(self):
        gui = load_fixture('gui', self.work)
        calls = []
        def case(cid, action):
            calls.append(cid)
            return cid != 'VM-DATA-001'
        with patch.object(gui, 'click'), self.assertRaisesRegex(RuntimeError, 'Policy import failed'):
            gui.run_cases(case, 'test-run')
        self.assertEqual(calls, ['VM-INSTALL-004', 'VM-INSTALL-003', 'VM-INSTALL-005',
                                 'VM-DATA-001'])

    def test_failed_restart_skips_dependent_browser_cases(self):
        browsers = load_fixture('browsers', self.work)
        calls = []
        def case(cid, action, browser=None):
            calls.append((cid, browser))
            return cid != 'VM-BR-002'
        with patch.object(browsers, 'setup'), patch.object(browsers, 'stop'), \
             patch.object(browsers, 'run', return_value=types.SimpleNamespace(stdout='test version')), \
             patch.object(browsers, 'dump', return_value={}), \
             patch.object(browsers, 'host_action'):
            browsers.run_cases(case, {'cases': []}, 'test-run')
        self.assertEqual(calls, [('VM-BR-001', 'firefox'), ('VM-BR-002', 'firefox'),
                                 ('VM-BR-001', 'chrome'), ('VM-BR-002', 'chrome')])


if __name__ == '__main__':
    unittest.main()
