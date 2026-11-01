"""Dispatch errors cannot substitute for an observed native UI transition."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


def load_controls():
    dependencies = {
        'pyatspi': SimpleNamespace(),
        'accessibility': SimpleNamespace(**{name: Mock() for name in
            ('apps', 'app_name', 'click', 'find', 'walk', 'refresh_desktop_cache')}),
        'guest': SimpleNamespace(host_action=Mock(), wait=Mock()),
    }
    spec = importlib.util.spec_from_file_location('observed_ui_controls',
        Path(__file__).parents[1] / 'fixtures/layer3/ui_controls.py')
    module = importlib.util.module_from_spec(spec)
    with patch.dict('sys.modules', dependencies):
        spec.loader.exec_module(module)
    return module


class ObservedTransitionTests(unittest.TestCase):
    def activation_controls(self, *, new_pid=101, active=True):
        controls = load_controls()
        controls.pyatspi.STATE_ACTIVE = 'active'
        before, after = Mock(), Mock()
        before.getApplication.return_value.get_process_id.return_value = 101
        after.getApplication.return_value.get_process_id.return_value = new_pid
        before.getState.return_value.contains.return_value = False
        after.getState.return_value.contains.return_value = active
        state = {'presented': False}
        controls.nodes = Mock(side_effect=lambda **kw: [after if state['presented'] else before])
        activate = Mock(side_effect=lambda: state.update(presented=True))
        controls.wait.side_effect = lambda ready, seconds: ready() or (_ for _ in ()).throw(TimeoutError())
        return controls, activate

    def test_named_activation_observes_the_original_process(self):
        controls, activate = self.activation_controls()
        controls.focus_app(activate=activate)
        activate.assert_called_once()
        controls.host_action.assert_not_called()

    def test_activation_cannot_mask_a_restarted_gui(self):
        controls, activate = self.activation_controls(new_pid=202)
        with self.assertRaisesRegex(AssertionError, 'original process'):
            controls.focus_app(activate=activate)

    def test_activation_acknowledgement_without_focus_is_a_failure(self):
        controls, activate = self.activation_controls(active=False)
        with self.assertRaises(TimeoutError):
            controls.focus_app(activate=activate)

    def test_native_fallback_reaches_the_original_window_behind_two_browsers(self):
        controls, activate = self.activation_controls(active=False)
        controls.apps.return_value = []
        selected, observations = [], []
        def switch(offset):
            selected.append(offset)
            frame = controls.nodes()[0]
            frame.getState.return_value.contains.return_value = offset == 2
        controls.focus_app(activate=activate, select_window=switch, report=observations.append)
        self.assertEqual(selected, [1, 2])
        self.assertEqual(observations[-1]['stage'], 'switch-2-focused')
        self.assertEqual(observations[-1]['original_pids'], [101])

    def test_native_fallback_never_accepts_a_different_process(self):
        controls, activate = self.activation_controls(active=False)
        def replaced(offset):
            controls.nodes()[0].getApplication.return_value.get_process_id.return_value = 202
        with self.assertRaisesRegex(AssertionError, 'original process'):
            controls.focus_app(activate=activate, select_window=replaced)

    def test_native_fallback_is_bounded_when_every_window_selection_is_ignored(self):
        controls, activate = self.activation_controls(active=False)
        switch = Mock()
        with self.assertRaisesRegex(TimeoutError, 'bounded window selection'):
            controls.focus_app(activate=activate, select_window=switch)
        self.assertEqual([call.args[0] for call in switch.call_args_list], list(range(1, 9)))

    def test_focus_rediscovers_a_live_frame_after_portal_cache_invalidation(self):
        controls = load_controls()
        controls.pyatspi.STATE_ACTIVE = 'active'
        frame = Mock()
        frame.getState.return_value.contains.return_value = True
        state = {'stale': True}
        controls.nodes = Mock(side_effect=lambda **kw: [] if state['stale'] else [frame])
        controls.refresh_desktop_cache.side_effect = lambda: state.update(stale=False)
        controls.wait.side_effect = lambda ready, seconds: ready() or (_ for _ in ()).throw(TimeoutError())
        controls.focus_app()
        controls.refresh_desktop_cache.assert_called_once()
        controls.host_action.assert_not_called()

    def test_cache_invalidation_cannot_substitute_for_a_live_frame(self):
        controls = load_controls()
        controls.nodes = Mock(return_value=[])
        controls.wait.side_effect = lambda ready, seconds: ready() or (_ for _ in ()).throw(TimeoutError())
        with self.assertRaises(TimeoutError):
            controls.focus_app()
        controls.refresh_desktop_cache.assert_called_once()
        controls.host_action.assert_not_called()

    def test_already_selected_option_does_not_open_a_native_popup(self):
        controls = load_controls()
        controls.pyatspi.STATE_SELECTED = 'selected'
        controls.pyatspi.STATE_SENSITIVE = 'sensitive'
        controls.pyatspi.STATE_EXPANDED = 'expanded'
        node, selected, other = Mock(), Mock(), Mock()
        node.getState.return_value.contains.side_effect = lambda state: state == 'sensitive'
        selected.name, other.name = 'Command', 'Path'
        for option, state in ((selected, True), (other, False)):
            option.getRoleName.return_value = 'menu item'
            option.getState.return_value.contains.return_value = state
        controls.walk.return_value = [selected, other]
        controls.nodes = Mock(side_effect=lambda **kw: [Mock()] if kw.get('role') == 'frame' else [])
        controls.wait.side_effect = lambda ready, seconds: ready()
        controls.choose(node, 'Command')
        controls.host_action.assert_not_called()
        node.queryComponent.assert_not_called()

    def selection_controls(self, *, commit=True, close=True, focus=True, report_expansion=True):
        controls = load_controls()
        for state in ('SELECTED', 'SENSITIVE', 'EXPANDED', 'FOCUSED', 'SHOWING', 'ACTIVE', 'SELECTABLE'):
            setattr(controls.pyatspi, 'STATE_' + state, state.lower())
        node = Mock()
        state = {'expanded': False, 'focused': False, 'selection': 0, 'pending': None}
        node.getState.return_value.contains.side_effect = lambda flag: (
            report_expansion and state['expanded'] if flag == 'expanded' else
            state.get(flag, flag in ('sensitive', 'showing')))
        def grab():
            state['focused'] = focus
            return True
        node.queryComponent.return_value.grabFocus.side_effect = grab
        options = [Mock(), Mock()]
        for index, (option, name) in enumerate(zip(options, ('Domain', 'Exact URL'))):
            option.name = name
            option.getRoleName.return_value = 'menu item'
            option.getState.return_value.contains.side_effect = lambda flag, i=index: (
                flag == 'selected' and state['selection'] == i)
        frame, popup = Mock(), Mock()
        popup.getState.return_value.contains.side_effect = lambda flag: flag == 'active'
        cells = [Mock(), Mock()]
        for cell, name in zip(cells, ('Domain', 'Exact URL')):
            cell.name = name
            cell.getRoleName.return_value = 'table cell'
            cell.getState.return_value.contains.side_effect = lambda flag: flag in ('showing', 'selectable')
        controls.walk.side_effect = lambda current: options if current is node else cells if current is popup else []
        controls.nodes = Mock(side_effect=lambda **kw: [frame] if kw.get('role') == 'frame' else
                              [popup] if kw.get('role') == 'window' and state['expanded'] else [])
        def keys(action, keys):
            self.assertEqual(action, 'keys')
            key = keys[0]
            if key == 'KEY_SPACE':
                state['pending'] = 'open'
            elif key in ('KEY_HOME', 'KEY_DOWN', 'KEY_ENTER'):
                self.assertTrue(state['expanded'], 'Navigated a closed native popup')
                if key == 'KEY_HOME': state['highlight'] = 0
                elif key == 'KEY_DOWN': state['highlight'] += 1
                else: state['pending'] = 'commit'
            elif key == 'KEY_TAB':
                self.assertFalse(state['expanded'], 'Moved on before popup closure')
                self.assertEqual(state['selection'], 1)
        def observed(ready, seconds):
            # Native input is asynchronous: the dispatch acknowledgement precedes
            # the accessibility transition, including committing the selection.
            ready()
            if state['pending'] == 'open': state['expanded'] = True
            elif state['pending'] == 'commit':
                state['expanded'] = not close
                if commit: state['selection'] = state['highlight']
            state['pending'] = None
            value = ready()
            if not value: raise TimeoutError('Native transition did not occur')
            return value
        controls.host_action.side_effect = keys
        controls.wait.side_effect = observed
        return controls, node, state

    def test_selection_waits_for_native_popup_and_committed_value(self):
        for report_expansion in (True, False):
            with self.subTest(report_expansion=report_expansion):
                controls, node, state = self.selection_controls(report_expansion=report_expansion)
                controls.choose(node, 'Exact URL')
                self.assertEqual(state['selection'], 1)
                self.assertFalse(state['expanded'])
                self.assertEqual(controls.host_action.call_args.kwargs['keys'], ['KEY_TAB'])

    def test_popup_without_expanded_flag_must_close_before_moving_on(self):
        controls, node, _ = self.selection_controls(close=False, report_expansion=False)
        with self.assertRaises(TimeoutError): controls.choose(node, 'Exact URL')
        self.assertNotIn(['KEY_TAB'], [call.kwargs['keys'] for call in controls.host_action.call_args_list])

    def test_unrelated_popup_cannot_substitute_for_the_select_menu(self):
        controls, node, state = self.selection_controls(report_expansion=False)
        state['expanded'] = True
        popup = controls.nodes(role='window')[0]
        controls.walk(popup)[0].name = 'Unrelated option'
        self.assertIsNone(controls.select_popup_state(node, ['Domain', 'Exact URL']))
        with self.assertRaises(TimeoutError): controls.choose(node, 'Exact URL')
        controls.host_action.assert_not_called()

    def test_popup_options_must_be_visible_and_its_window_active(self):
        controls, node, state = self.selection_controls(report_expansion=False)
        state['expanded'] = True
        popup = controls.nodes(role='window')[0]
        popup.getState.return_value.contains.side_effect = lambda flag: False
        self.assertEqual(controls.select_popup_state(node, ['Domain', 'Exact URL']), 'closed')
        popup.getState.return_value.contains.side_effect = lambda flag: flag == 'active'
        controls.walk(popup)[0].getState.return_value.contains.side_effect = lambda flag: flag == 'selectable'
        self.assertIsNone(controls.select_popup_state(node, ['Domain', 'Exact URL']))

    def test_already_selected_open_popup_is_committed_and_closed(self):
        controls, node, state = self.selection_controls(report_expansion=False)
        state.update(expanded=True, selection=1, highlight=1)
        controls.choose(node, 'Exact URL')
        self.assertFalse(state['expanded'])
        self.assertEqual(state['selection'], 1)
        controls.host_action.assert_called_once_with('keys', keys=['KEY_ENTER'])

    def test_unavailable_accessibility_tree_does_not_prove_popup_closure(self):
        controls, node, _ = self.selection_controls(report_expansion=False)
        controls.nodes.return_value = []
        controls.nodes.side_effect = None
        self.assertIsNone(controls.select_popup_state(node, ['Domain', 'Exact URL']))
        with self.assertRaises(TimeoutError): controls.choose(node, 'Exact URL')
        controls.host_action.assert_not_called()

    def test_popup_closure_without_requested_selection_is_a_failure(self):
        controls, node, state = self.selection_controls(commit=False)
        with self.assertRaises(TimeoutError): controls.choose(node, 'Exact URL')
        self.assertNotIn(['KEY_TAB'], [call.kwargs['keys'] for call in controls.host_action.call_args_list])

    def test_successful_focus_request_without_actual_focus_cannot_send_keys(self):
        controls, node, _ = self.selection_controls(focus=False)
        with self.assertRaises(TimeoutError): controls.choose(node, 'Exact URL')
        controls.host_action.assert_not_called()

    def button_controls(self, sensitive=True, focus=True):
        controls = load_controls()
        controls.pyatspi.STATE_SENSITIVE = 'sensitive'
        controls.pyatspi.STATE_SHOWING = 'showing'
        node = Mock()
        shown = False
        node.getState.return_value.contains.side_effect = lambda flag: sensitive if flag == 'sensitive' else shown
        def grab():
            nonlocal shown
            shown = focus
            return focus
        node.queryComponent.return_value.grabFocus.side_effect = grab
        def observed(ready, seconds):
            value = ready()
            if not value:
                raise TimeoutError('Control stayed outside the viewport')
            return value
        controls.find.return_value = node
        controls.wait.side_effect = observed
        return controls, node

    def test_offscreen_button_is_focused_and_activated_through_native_input(self):
        controls, node = self.button_controls()
        node.queryAction.side_effect = RuntimeError('Synchronous dispatch is unavailable')
        controls.press('New application')
        controls.host_action.assert_called_once_with('keys', keys=['KEY_ENTER'])
        node.queryAction.assert_not_called()

    def test_disabled_button_cannot_dispatch_native_input(self):
        controls, node = self.button_controls(sensitive=False)
        with self.assertRaisesRegex(AssertionError, 'disabled'):
            controls.press('New application')
        controls.host_action.assert_not_called()

    def test_button_without_focus_cannot_dispatch_native_input(self):
        controls, node = self.button_controls(focus=False)
        with self.assertRaisesRegex(AssertionError, 'focus'):
            controls.press('New application')
        controls.host_action.assert_not_called()

    def test_focus_uses_current_frame_after_a_modal_replaces_it(self):
        controls = load_controls()
        controls.pyatspi.STATE_ACTIVE = 'active'
        stale, current = Mock(), Mock()
        stale.getState.return_value.contains.return_value = False
        current.getState.return_value.contains.return_value = True
        def observed(ready, seconds):
            value = ready()
            if not value:
                raise TimeoutError('No active target frame')
            return value
        with patch.object(controls, 'nodes', side_effect=[[stale], [current]]), \
                patch.object(controls, 'wait', side_effect=observed):
            controls.focus_app()
        controls.host_action.assert_not_called()
        stale.queryComponent.assert_not_called()

    def test_native_switching_cannot_accept_an_inactive_target_window(self):
        controls = load_controls()
        controls.pyatspi.STATE_ACTIVE = 'active'
        inactive = Mock()
        inactive.getState.return_value.contains.return_value = False
        def observed(ready, seconds):
            value = ready()
            if not value:
                raise TimeoutError('No active target frame')
            return value
        with patch.object(controls, 'nodes', return_value=[inactive]), \
                patch.object(controls, 'wait', side_effect=observed):
            with self.assertRaisesRegex(TimeoutError, 'active target'):
                controls.focus_app()
        self.assertLessEqual(controls.host_action.call_count, 3)

    def test_overview_dismissal_must_activate_the_target_window(self):
        controls = load_controls()
        controls.pyatspi.STATE_ACTIVE = 'active'
        frame = Mock()
        active = False
        frame.getState.return_value.contains.side_effect = lambda flag: active
        def observed(ready, seconds):
            value = ready()
            if not value:
                raise TimeoutError('No active target frame')
            return value
        def dismiss(action, keys):
            nonlocal active
            self.assertEqual(keys, ['KEY_ESC'])
            active = True
        controls.host_action.side_effect = dismiss
        with patch.object(controls, 'nodes', return_value=[frame]), \
                patch.object(controls, 'wait', side_effect=observed):
            controls.focus_app()
        controls.host_action.assert_called_once_with('keys', keys=['KEY_ESC'])

    def test_dispatch_timeout_needs_the_actual_expected_transition(self):
        controls = load_controls()
        def observed(ready, seconds):
            self.assertEqual(seconds, 15)
            if not ready():
                raise TimeoutError('Expected controls never appeared')
            return True
        with patch.object(controls, 'press', side_effect=RuntimeError('Did not receive a reply')), \
                patch.object(controls, 'wait', side_effect=observed):
            self.assertTrue(controls.press_observed('Show identities', lambda: True))
            with self.assertRaisesRegex(TimeoutError, 'never appeared'):
                controls.press_observed('Show identities', lambda: False)

    def test_unrelated_action_error_is_not_hidden_by_existing_controls(self):
        controls = load_controls()
        with patch.object(controls, 'press', side_effect=RuntimeError('Control is disabled')):
            with self.assertRaisesRegex(RuntimeError, 'disabled'):
                controls.press_observed('Show identities', lambda: True)
        controls.wait.assert_not_called()

    def test_successful_dispatch_still_requires_the_transition(self):
        controls = load_controls()
        with patch.object(controls, 'press'), \
                patch.object(controls, 'wait', side_effect=TimeoutError('No transition')):
            with self.assertRaisesRegex(TimeoutError, 'No transition'):
                controls.press_observed('Show identities', lambda: False)


if __name__ == '__main__':
    unittest.main()
