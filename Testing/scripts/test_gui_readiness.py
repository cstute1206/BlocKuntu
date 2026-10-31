"""UI readiness and rejection semantics must be observed before advancing."""
import ast
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).parents[1]


def load_gui():
    dependencies = {
        'pyatspi': SimpleNamespace(STATE_SENSITIVE='sensitive'),
        'accessibility': SimpleNamespace(**{name: Mock() for name in
            ('app_name', 'apps', 'click', 'dump', 'find', 'texts', 'walk')}),
        'guest': SimpleNamespace(WORK=Path('/unused'), **{name: Mock() for name in
            ('host_action', 'rpc', 'run', 'wait')}),
        'ui_controls': SimpleNamespace(**{name: Mock() for name in
            ('focus_app', 'nodes', 'press_observed')}),
    }
    spec = importlib.util.spec_from_file_location('readiness_gui', ROOT / 'fixtures/layer3/gui.py')
    module = importlib.util.module_from_spec(spec)
    with patch.dict('sys.modules', dependencies):
        spec.loader.exec_module(module)
    return module


class ImportReadinessTests(unittest.TestCase):
    def test_disabled_idle_label_does_not_prove_completion(self):
        gui = load_gui()
        button = Mock()
        button.getState.return_value.contains.return_value = False
        gui.nodes.return_value = [button]
        self.assertIsNone(gui.import_idle())
        button.getState.return_value.contains.return_value = True
        self.assertIs(gui.import_idle(), button)

    def test_welcome_uses_focus_and_observed_dismissal(self):
        gui = load_gui()
        gui.leave_desktop_overview = Mock()
        gui.nodes.return_value = []
        gui.press_observed.side_effect = lambda name, ready: self.assertTrue(ready())
        gui.dismiss_welcome('fedora')
        gui.leave_desktop_overview.assert_called_once_with('fedora')
        gui.focus_app.assert_called_once()
        gui.press_observed.assert_called_once()
        gui.click.assert_not_called()

    def test_success_text_waits_for_closed_chooser_and_idle_refresh(self):
        gui = load_gui()
        state = {'chooser': False, 'completed_polls': 0}
        gui.visible_chooser = lambda title: 'chooser-app' if state['chooser'] else None
        gui.texts.return_value = 'Policy appended from previous action'
        gui.import_idle = Mock(side_effect=lambda: (not state['chooser'] and
            (state['completed_polls'] == 0 or state['completed_polls'] >= 3)))
        def observed(ready, seconds):
            for _ in range(5):
                value = ready()
                if value:
                    return value
                state['completed_polls'] += 1
            raise TimeoutError('not ready')
        gui.wait.side_effect = observed
        def press(name, ready):
            state['chooser'] = True
            return ready()
        gui.press_observed.side_effect = press
        gui.leave_desktop_overview = Mock()
        gui.chooser_editable = Mock()
        gui.chooser_item = Mock()
        def close(*args):
            state.update(chooser=False, completed_polls=1)
        gui.chooser_button = close
        gui.choose_policy_import(Path('/fixture/import.toml'), 'ubuntu', 'Policy appended from')
        self.assertEqual(state['completed_polls'], 3)
        self.assertFalse(state['chooser'])
        self.assertEqual(gui.focus_app.call_count, 2)

    def test_readiness_failure_keeps_stage_and_accessibility_evidence(self):
        gui = load_gui()
        gui.visible_chooser = lambda title: None
        gui.wait.side_effect = TimeoutError('import stayed busy')
        gui.dump.return_value = [{'role': 'button', 'name': 'Appending'}]
        with tempfile.TemporaryDirectory() as directory:
            gui.WORK = Path(directory)
            with self.assertRaisesRegex(TimeoutError, 'stayed busy'):
                gui.choose_policy_import(Path('/fixture/import.toml'), 'ubuntu', 'success')
            evidence = json.loads((gui.WORK / 'gui-import-failure.json').read_text())
            self.assertEqual(evidence['stage'], 'ready')
            self.assertEqual(evidence['applications']['blockuntu-gui'][0]['name'], 'Appending')
        gui.host_action.assert_called_once_with('screenshot', name='gui-import-failure')


class ProtectedUninstallTests(unittest.TestCase):
    def rejection(self, kind, message=None):
        tree = ast.parse((ROOT / 'fixtures/layer4/uninstall.py').read_text())
        function = next(n for n in tree.body if isinstance(n, ast.FunctionDef)
                        and n.name == 'protected_rejection')
        namespace = {name: Mock() for name in ('fresh_protected', 'combo', 'find', 'type_phrase',
                     'press_uninstall', 'no_lease', 'texts', 'rpc', 'wait', 'enabled')}
        namespace.update(dt=__import__('datetime'), redact=str)
        namespace['enabled'].return_value = False
        namespace['no_lease'].return_value = True
        namespace['texts'].return_value = 'Uninstall is unavailable until all active Detox sessions end.'
        namespace['wait'].side_effect = lambda ready, seconds: ready() or (_ for _ in ()).throw(TimeoutError())
        def rpc(method):
            if method == 'tier1_edit_status': return {'protected_access_open': False}
            if method == 'config_snapshot': return {'policy': 'unchanged'}
            if method == 'status': return {'enforcement_state': 'active'}
            raise RuntimeError(message or 'Uninstall is unavailable until all active Detox sessions end.')
        namespace['rpc'].side_effect = rpc
        exec(compile(ast.Module(body=[function], type_ignores=[]), '<protected uninstall>', 'exec'), namespace)
        return namespace

    def test_detox_requires_disabled_ui_and_specific_daemon_rejection(self):
        ns = self.rejection('detox')
        result = ns['protected_rejection']('detox')
        self.assertTrue(result['gui_uninstall_rejected'])
        self.assertTrue(result['rpc_uninstall_rejected'])
        ns['press_uninstall'].assert_not_called()
        ns['type_phrase'].assert_not_called()

    def test_unrelated_rpc_failure_cannot_pass_detox_rejection(self):
        ns = self.rejection('detox', 'socket transport failed')
        with self.assertRaises(AssertionError):
            ns['protected_rejection']('detox')

    def test_enabled_uninstall_button_during_detox_is_a_failure(self):
        ns = self.rejection('detox')
        ns['enabled'].side_effect = [False, True]
        with self.assertRaises(TimeoutError):
            ns['protected_rejection']('detox')


if __name__ == '__main__':
    unittest.main()
