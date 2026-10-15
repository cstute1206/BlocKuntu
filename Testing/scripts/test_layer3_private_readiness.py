"""Mode changes require daemon permission and refreshed GUI availability."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from test_layer3_dependencies import load_fixture


class PrivateReadinessTests(unittest.TestCase):
    def fixture(self, *, daemon_ready=True, gui_ready=True):
        browsers=SimpleNamespace(APPS={},BROWSER_CONFIG={},**{name:Mock() for name in
            ('start','stop','navigate','heartbeat')})
        controls=SimpleNamespace(**{name:Mock() for name in
            ('activate','choose','enabled','focus_app','nodes','open_settings')})
        with patch.dict('sys.modules',browsers=browsers,ui_controls=controls):
            private=load_fixture('private_browsing','/unused')
        responses=iter([False,False,daemon_ready])
        private.rpc=Mock(side_effect=lambda *args: {'chromium_incognito_settings_change_allowed':next(responses,daemon_ready)})
        old,new=SimpleNamespace(name='Chromium private browsing',enabled=False),SimpleNamespace(name='Chromium private browsing',enabled=gui_ready)
        rows=iter([[old],[old],[new]])
        private.nodes=Mock(side_effect=lambda **kwargs:next(rows,[new]))
        private.enabled=lambda node:node.enabled
        private.open_settings=Mock()
        def wait(ready,seconds):
            self.assertEqual(seconds,30)
            for _ in range(3):
                result=ready()
                if result:return result
            raise TimeoutError('Permission or GUI readiness never arrived')
        private.wait=wait
        return private,new

    def test_returns_refreshed_control_after_daemon_and_gui_unlock(self):
        private,new=self.fixture()
        self.assertIs(private.editable_private_settings(),new)
        self.assertEqual(private.rpc.call_count,3)
        self.assertEqual(private.nodes.call_count,3)
        private.open_settings.assert_called_once_with('Protected changes and uninstall')

    def test_daemon_lock_prevents_gui_mode_change_setup(self):
        private,_=self.fixture(daemon_ready=False)
        with self.assertRaisesRegex(TimeoutError,'never arrived'):
            private.editable_private_settings()
        private.open_settings.assert_not_called()
        private.nodes.assert_not_called()

    def test_gui_lock_is_not_overridden_by_daemon_permission(self):
        private,_=self.fixture(gui_ready=False)
        with self.assertRaisesRegex(TimeoutError,'never arrived'):
            private.editable_private_settings()


if __name__=='__main__':unittest.main()
