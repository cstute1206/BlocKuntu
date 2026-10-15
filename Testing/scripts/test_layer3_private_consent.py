"""Private consent must observe the selected state of the browser-owned control."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from test_layer3_dependencies import load_fixture


class PrivateConsentStateTests(unittest.TestCase):
    def setUp(self):
        browsers=SimpleNamespace(APPS={},BROWSER_CONFIG={},**{name:Mock() for name in
            ('start','stop','navigate','heartbeat')})
        controls=SimpleNamespace(**{name:Mock() for name in
            ('activate','choose','enabled','focus_app','nodes','open_settings')})
        with patch.dict('sys.modules',browsers=browsers,ui_controls=controls):
            self.private=load_fixture('private_browsing','/unused')
        self.private.pyatspi=SimpleNamespace(STATE_CHECKED='checked',STATE_PRESSED='pressed')

    def consent(self,role,*states):
        node=Mock();node.getRoleName.return_value=role
        node.getState.return_value.contains.side_effect=lambda state:state in states
        return self.private.incognito_consent_enabled(node)

    def test_checked_browser_control_is_consent(self):
        for role in ('check box','switch','toggle button'):
            with self.subTest(role=role):self.assertTrue(self.consent(role,'checked'))

    def test_legacy_pressed_toggle_is_consent(self):
        self.assertTrue(self.consent('toggle button','pressed'))

    def test_unselected_toggle_is_not_consent(self):
        self.assertFalse(self.consent('toggle button'))

    def test_pressed_does_not_substitute_for_checked_on_other_roles(self):
        for role in ('check box','switch','button'):
            with self.subTest(role=role):self.assertFalse(self.consent(role,'pressed'))

    def test_focus_or_enabled_state_is_not_consent(self):
        self.assertFalse(self.consent('toggle button','focused','enabled','sensitive'))


if __name__=='__main__':unittest.main()
