"""Desktop setup must select GNOME or verify KDE accessibility and layout."""
import importlib.util
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('desktop_guest_setup',Path(__file__).resolve().parents[1]/'fixtures/layer3/guest.py')
guest=importlib.util.module_from_spec(spec);spec.loader.exec_module(guest)


class DesktopSetupTests(unittest.TestCase):
    def test_gnome_guests_keep_the_same_accessibility_and_keyboard_setup(self):
        calls=[]
        with patch.object(guest,'run',side_effect=lambda *args: calls.append(args)):
            for name in ('ubuntu','fedora'):
                before=len(calls);guest.configure_desktop(name)
                self.assertEqual(len(calls)-before,5)
                self.assertIn(('gsettings','set','org.gnome.desktop.input-sources','sources',"[('xkb', 'us')]"),calls)

    def test_kde_requires_actual_accessibility_and_us_layout(self):
        calls=[]
        def run(*args):
            calls.append(args)
            value='(<true>,)' if 'Properties.Get' in ' '.join(args) else 'a(sss) 1 "us" "" "English"' if args[-1]=='getLayoutsList' else 'u 0'
            return SimpleNamespace(stdout=value)
        with tempfile.TemporaryDirectory() as directory,patch.object(guest,'WORK',Path(directory)),patch.object(guest,'run',side_effect=run):
            guest.configure_desktop('cachyos')
            self.assertFalse(any(c[0]=='gsettings' for c in calls))
            self.assertTrue((Path(directory)/'keyboard-layout.txt').exists())
        with patch.object(guest,'run',return_value=SimpleNamespace(stdout='(<false>,)')):
            with self.assertRaisesRegex(AssertionError,'accessibility'):
                guest.configure_desktop('cachyos')


if __name__=='__main__':unittest.main()
