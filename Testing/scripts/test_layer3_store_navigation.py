"""Store onboarding survives a welcome tab without overwriting the fixture tab."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from test_layer3_dependencies import load_fixture


class StoreNavigationTests(unittest.TestCase):
    def test_extra_launch_tab_cannot_replace_observed_store_navigation(self):
        for browser in ('chrome','chromium'):
            with self.subTest(browser=browser):
                module=load_fixture('browsers','/unused')
                tabs=['fixture','consent','welcome'];active=0
                def keys(action,keys):
                    nonlocal active
                    self.assertEqual((action,keys),('keys',['KEY_LEFTCTRL','KEY_2']))
                    active=2  # A launch tab steals focus after native tab selection.
                def navigate(name,url):
                    self.assertEqual(name,browser)
                    tabs[active]=url
                def consent(app):
                    self.assertEqual(app,module.APPS[browser])
                    self.assertEqual(tabs[active],module.STORES[browser])
                module.host_action=Mock(side_effect=keys)
                module.navigate=Mock(side_effect=navigate)
                module.dismiss_google_consent=Mock(side_effect=consent)
                module.select_tab=Mock(side_effect=AssertionError('AT-SPI acknowledgement is not a native tab transition'))
                focus=Mock()
                with patch.dict('sys.modules',ui_controls=SimpleNamespace(focus_app=focus)):
                    module.select_chromium_store(browser)
                self.assertEqual(tabs[0],'fixture')
                focus.assert_called_once_with(module.APPS[browser])
                module.navigate.assert_called_once_with(browser,module.STORES[browser])

    def test_chromium_store_helper_rejects_other_browser_families(self):
        module=load_fixture('browsers','/unused')
        module.host_action=Mock()
        with self.assertRaises(ValueError):module.select_chromium_store('firefox')
        module.host_action.assert_not_called()


if __name__=='__main__':unittest.main()
