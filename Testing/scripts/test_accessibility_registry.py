"""A stale portal entry must not discard the remaining desktop applications."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


def load_registry(desktop):
    pyatspi = SimpleNamespace(Atspi=SimpleNamespace(set_timeout=Mock()),
                              Registry=SimpleNamespace(getDesktop=Mock(return_value=desktop)))
    spec = importlib.util.spec_from_file_location('registry_accessibility',
        Path(__file__).parents[1] / 'fixtures/layer3/accessibility.py')
    module = importlib.util.module_from_spec(spec)
    with patch.dict('sys.modules', {'pyatspi': pyatspi}):
        spec.loader.exec_module(module)
    return module


class RegistryTests(unittest.TestCase):
    def test_closing_portal_entry_does_not_hide_live_gui(self):
        desktop = Mock(childCount=3)
        shell, gui = object(), object()
        desktop.getChildAtIndex.side_effect = [shell, RuntimeError('portal closed'), gui]
        registry = load_registry(desktop)
        self.assertEqual(registry.apps(), [shell, gui])

    def test_missing_desktop_is_not_reported_as_an_application(self):
        registry = load_registry(Mock())
        registry.pyatspi.Registry.getDesktop.side_effect = RuntimeError('bus unavailable')
        self.assertEqual(registry.apps(), [])

    def test_refresh_invalidates_the_actual_desktop_cache(self):
        desktop = Mock()
        registry = load_registry(desktop)
        registry.refresh_desktop_cache()
        desktop.clearCache.assert_called_once()
