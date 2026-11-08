"""Private navigation must target its own address bar and active window."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from test_layer3_dependencies import load_fixture
from test_layer3_ui_observation import load_controls


class WindowNavigationTests(unittest.TestCase):
    def fixture(self, *, activate=True, address_bar=True, obscured=False):
        browser=load_fixture('browsers','/unused')
        browser.pyatspi=SimpleNamespace(STATE_SHOWING='showing',STATE_FOCUSED='focused',STATE_ACTIVE='active')
        window, target=Mock(),Mock()
        focused=False
        active=activate and not obscured
        window.getState.return_value.contains.side_effect=lambda state: active
        def raise_app(app):
            nonlocal active
            self.assertEqual(app,'Google Chrome')
            active=activate
        raise_window=Mock(side_effect=raise_app)
        patcher=patch.dict('sys.modules',ui_controls=SimpleNamespace(focus_app=raise_window))
        patcher.start();self.addCleanup(patcher.stop)
        browser.raise_window=raise_window
        target.name='Address and search bar'
        target.getRoleName.return_value='entry'
        target.getState.return_value.contains.side_effect=lambda state: state=='showing' or focused
        def focus():
            nonlocal focused
            focused=True
            return True
        target.queryComponent.return_value.grabFocus.side_effect=focus
        browser.walk=Mock(side_effect=lambda owner: [target] if owner is window and address_bar else [])
        browser.find=Mock(side_effect=AssertionError('Must not select a different window through the global tree'))
        def wait(ready,seconds):
            result=ready()
            if not result:raise TimeoutError('Target window/address bar unavailable')
            return result
        browser.wait=wait
        browser.host_action=Mock()
        return browser,window,target

    def test_native_navigation_uses_only_the_asserted_window(self):
        browser,window,target=self.fixture()
        browser.navigate('chrome','http://web.blockuntu.test:18080/free',window=window)
        browser.walk.assert_called_once_with(window)
        target.queryComponent.return_value.grabFocus.assert_called_once_with()
        browser.find.assert_not_called()
        browser.host_action.assert_called_once_with('type_url',url='http://web.blockuntu.test:18080/free')
        browser.raise_window.assert_not_called()

    def test_gui_obscured_browser_is_raised_before_scoped_input(self):
        browser,window,target=self.fixture(obscured=True)
        def keys(*args,**kwargs):
            self.assertTrue(window.getState().contains('active'))
            self.assertTrue(target.getState().contains('focused'))
        browser.host_action.side_effect=keys
        browser.navigate('chrome','http://web.blockuntu.test:18080/free',window=window)
        browser.raise_window.assert_called_once_with('Google Chrome')
        browser.host_action.assert_called_once()

    def test_focus_ack_without_active_target_cannot_send_input(self):
        browser,window,_=self.fixture(activate=False)
        with self.assertRaisesRegex(TimeoutError,'Target window'):
            browser.navigate('chrome','http://web.blockuntu.test:18080/free',window=window)
        browser.host_action.assert_not_called()
        browser.raise_window.assert_called_once_with('Google Chrome')

    def test_missing_target_address_bar_cannot_fall_back_to_other_window(self):
        browser,window,_=self.fixture(address_bar=False)
        with self.assertRaisesRegex(TimeoutError,'address bar'):
            browser.navigate('chrome','http://web.blockuntu.test:18080/free',window=window)
        browser.find.assert_not_called()
        browser.host_action.assert_not_called()

    def test_regular_navigation_selects_visible_bar_and_observes_focus(self):
        browser,_,target=self.fixture()
        browser.find=Mock(return_value=target)
        browser.navigate('chrome','http://web.blockuntu.test:18080/free')
        browser.raise_window.assert_called_once_with('Google Chrome')
        browser.find.assert_called_once_with('Address and search bar','entry','Google Chrome',visible=True)
        self.assertEqual(browser.host_action.call_args_list,[
            unittest.mock.call('keys',keys=['KEY_LEFTCTRL','KEY_L']),
            unittest.mock.call('type_url',url='http://web.blockuntu.test:18080/free')])

    def test_regular_focus_ack_without_focused_bar_cannot_type_url(self):
        browser,_,target=self.fixture()
        browser.find=Mock(return_value=target)
        target.queryComponent.return_value.grabFocus.side_effect=None
        target.queryComponent.return_value.grabFocus.return_value=True
        with self.assertRaises(TimeoutError):
            browser.navigate('chrome','http://web.blockuntu.test:18080/free')
        browser.host_action.assert_called_once_with('keys',keys=['KEY_LEFTCTRL','KEY_L'])


class BrowserRecoveryFocusTests(unittest.TestCase):
    """Exercise real navigation/focus helpers against native MRU ordering."""

    def fixture(self, *, replace_process=False, ignore_selection=False):
        browser = load_fixture('browsers', '/unused')
        controls = load_controls()
        flags = SimpleNamespace(STATE_ACTIVE='active', STATE_SHOWING='showing', STATE_FOCUSED='focused')
        controls.pyatspi = browser.pyatspi = flags
        order = ['Firefox', 'Ubuntu', 'Google Chrome']
        applications, frames = {}, {}
        for pid, name in enumerate(order, 101):
            application, frame = Mock(), Mock()
            application.name = name
            application.get_process_id.return_value = pid
            application.childCount = 1
            application.getChildAtIndex.return_value = frame
            frame.name = name + ' window'
            frame.getApplication.return_value = application
            frame.getRoleName.return_value = 'frame'
            frame.getState.return_value.contains.side_effect = lambda flag, app=name: (
                flag == 'showing' or flag == 'active' and order[0] == app)
            applications[name], frames[name] = application, frame
        controls.apps.return_value = list(applications.values())
        controls.app_name.side_effect = lambda application: application.name
        controls.nodes = Mock(side_effect=lambda app='blockuntu-gui', **kw: [frames[app]])
        def wait(ready, seconds):
            value = ready()
            if not value:
                raise TimeoutError('Required native transition did not occur')
            return value
        controls.wait.side_effect = wait
        browser.wait = wait
        selections = []
        def native(action, **request):
            if action == 'keys':
                self.assertEqual(request['keys'], ['KEY_ESC'])
                return
            self.assertEqual(action, 'select_gui_window')
            offset = request['steps']
            selections.append(offset)
            if replace_process:
                applications['Google Chrome'].get_process_id.return_value = 999
            if not ignore_selection:
                selected = order[offset % len(order)]
                order.remove(selected)
                order.insert(0, selected)
        controls.host_action.side_effect = native
        bar = Mock()
        bar.getState.return_value.contains.side_effect = lambda flag: (
            flag == 'showing' or flag == 'focused' and order[0] == 'Google Chrome')
        bar.queryComponent.return_value.grabFocus.side_effect = lambda: order[0] == 'Google Chrome'
        browser.find = Mock(return_value=bar)
        def browser_input(action, **request):
            self.assertEqual(order[0], 'Google Chrome', 'Input went to an unrelated application')
        browser.host_action = Mock(side_effect=browser_input)
        patcher = patch.dict('sys.modules', ui_controls=controls)
        patcher.start()
        self.addCleanup(patcher.stop)
        return browser, controls, order, selections

    def test_recovery_reaches_original_browser_behind_two_other_applications(self):
        browser, controls, order, selections = self.fixture()
        observations = []
        self.assertEqual(order, ['Firefox', 'Ubuntu', 'Google Chrome'])
        browser.navigate('chrome', 'chrome://extensions',
                         select_window=lambda offset: controls.host_action('select_gui_window', steps=offset),
                         report=observations.append)
        self.assertEqual(selections, [1, 2])
        self.assertEqual(order[0], 'Google Chrome')
        self.assertEqual(observations[-1]['stage'], 'switch-2-focused')
        self.assertEqual(observations[-1]['original_pids'], [103])
        self.assertEqual(observations[2]['active_windows'][0]['app'], 'Ubuntu')
        browser.host_action.assert_any_call('type_url', url='chrome://extensions')

    def test_replaced_browser_cannot_receive_recovery_input(self):
        browser, controls, _, _ = self.fixture(replace_process=True)
        with self.assertRaisesRegex(AssertionError, 'original process'):
            browser.navigate('chrome', 'chrome://extensions',
                             select_window=lambda offset: controls.host_action('select_gui_window', steps=offset))
        browser.host_action.assert_not_called()

    def test_ignored_selection_is_bounded_and_does_not_type_into_another_app(self):
        browser, controls, order, selections = self.fixture(ignore_selection=True)
        with self.assertRaisesRegex(TimeoutError, 'Google Chrome.*bounded window selection'):
            browser.navigate('chrome', 'chrome://extensions',
                             select_window=lambda offset: controls.host_action('select_gui_window', steps=offset))
        self.assertEqual(selections, list(range(1, 9)))
        self.assertEqual(order[0], 'Firefox')
        browser.host_action.assert_not_called()


if __name__=='__main__':unittest.main()
