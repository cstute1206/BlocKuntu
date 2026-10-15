"""Consent requires native focus and an observed transition to the real store."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from test_layer3_dependencies import load_fixture


class StoreConsentTests(unittest.TestCase):
    def fixture(self, *, sensitive=True, focus=True, transition=True, at_store=False):
        browser=load_fixture('browsers', '/unused')
        browser.pyatspi=SimpleNamespace(STATE_SENSITIVE='sensitive', STATE_FOCUSED='focused', STATE_SHOWING='showing')
        button=Mock()
        focused=False
        def state(flag):
            return sensitive if flag=='sensitive' else focused
        button.getState.return_value.contains.side_effect=state
        def grab():
            nonlocal focused
            focused=focus
            return focus
        button.queryComponent.return_value.grabFocus.side_effect=grab
        button.queryAction.side_effect=RuntimeError('AT-SPI activation must not be used')
        store=at_store
        browser.visible_document_text=Mock(side_effect=lambda app:
            'BlocKuntu - Chrome Web Store' if store else 'Before you continue to Google')
        browser.find=Mock(return_value=button)
        def wait(ready, seconds):
            result=ready()
            if not result:raise TimeoutError('Required transition never happened')
            return result
        browser.wait=wait
        def keys(action, keys):
            nonlocal store
            self.assertTrue(focused)
            self.assertEqual((action, keys), ('keys', ['KEY_ENTER']))
            store=transition
        browser.host_action=Mock(side_effect=keys)
        return browser, button

    def run_consent(self, browser):
        with patch.dict('sys.modules', ui_controls=SimpleNamespace(focus_app=Mock())):
            return browser.dismiss_google_consent('Google Chrome')

    def test_native_submit_must_reach_store(self):
        browser, button=self.fixture()
        self.assertTrue(self.run_consent(browser))
        browser.host_action.assert_called_once_with('keys', keys=['KEY_ENTER'])
        button.queryAction.assert_not_called()

    def test_dispatch_without_store_transition_is_rejected(self):
        browser, _=self.fixture(transition=False)
        with self.assertRaisesRegex(TimeoutError, 'never happened'):
            self.run_consent(browser)

    def test_disabled_consent_cannot_send_input(self):
        browser, _=self.fixture(sensitive=False)
        with self.assertRaisesRegex(AssertionError, 'disabled'):
            self.run_consent(browser)
        browser.host_action.assert_not_called()

    def test_unfocused_consent_cannot_send_input(self):
        browser, _=self.fixture(focus=False)
        with self.assertRaisesRegex(AssertionError, 'focus'):
            self.run_consent(browser)
        browser.host_action.assert_not_called()

    def test_existing_store_page_does_not_change_consent(self):
        browser, _=self.fixture(at_store=True)
        self.assertFalse(self.run_consent(browser))
        browser.find.assert_not_called()
        browser.host_action.assert_not_called()


if __name__=='__main__':unittest.main()
