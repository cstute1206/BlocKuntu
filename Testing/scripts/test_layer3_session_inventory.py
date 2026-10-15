"""Keep non-dumpable and setuid processes in the correct real-user inventory."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('session_inventory',Path(__file__).parents[1]/'fixtures/layer3/session_inventory.py')
inventory=importlib.util.module_from_spec(spec);spec.loader.exec_module(inventory)


class SessionInventoryTests(unittest.TestCase):
    def fixture(self,root,pid,uid,effective_uid,exe):
        p=root/str(pid);p.mkdir()
        (p/'status').write_text(f'Name:\tfixture\nUid:\t{uid}\t{effective_uid}\t{effective_uid}\t{uid}\n')
        (p/'stat').write_text(f'{pid} (fixture with spaces) S 42 '+' '.join(['0']*17+['98765'])+'\n')
        (p/'exe').symlink_to(exe)
        return p

    def test_real_uid_is_independent_of_directory_owner_and_effective_uid(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            # GitHub's runner owns its files as UID 1001. The simulated real UID
            # must differ from the actual owner on every host.
            real_uid=1002 if root.stat().st_uid==1001 else 1001
            p=self.fixture(root,123,real_uid,0,'/usr/lib/systemd/systemd')
            self.assertNotEqual(p.stat().st_uid,real_uid)
            row=inventory.collect_processes(root)[123]
            self.assertEqual(row['uid'],real_uid)
            self.assertEqual(row['exe'],'/usr/lib/systemd/systemd')

    def test_correlated_pid_parent_and_start_identity_survive_comm_spaces(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);self.fixture(root,123,1000,1000,'/test/bk-test-parent')
            self.assertEqual(inventory.collect_processes(root)[123],
                {'uid':1000,'exe':'/test/bk-test-parent','ppid':42,'start_ticks':'98765'})

    def test_vanished_process_and_non_pid_entries_are_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'self').mkdir();(root/'456').mkdir()
            self.fixture(root,123,1000,1000,'/test/known')
            self.assertEqual(set(inventory.collect_processes(root)),{123})


if __name__=='__main__':unittest.main()
