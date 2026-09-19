"""Exercise the destructive-operation boundary without touching libvirt."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET

spec = importlib.util.spec_from_file_location("phase0_vm", Path(__file__).with_name("phase0-vm.py"))
vm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vm)


class CloneGuardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=vm.ROOT / "runtime")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.directory = self.root / "run" / "ubuntu"
        self.directory.mkdir(parents=True)
        self.disk = self.directory / "disk.qcow2"
        self.disk.write_bytes(b"clone")
        self.base_disk = self.root / "base.qcow2"
        self.base_disk.write_bytes(b"base")
        self.record = {"run_id": "run", "template": "ubuntu", "domain": "clone",
                       "uuid": "clone-uuid", "paths": [str(self.disk)]}
        self.xml = ET.fromstring(f'<domain><name>clone</name><uuid>clone-uuid</uuid><devices><disk type="file" device="disk"><source file="{self.disk}"/></disk></devices></domain>')
        self.bases = {"ubuntu": {"uuid": "base-uuid", "storage": {str(self.base_disk): []}}}
        self.record["bases"] = self.bases
        self.info = {"format": "qcow2"}
        self.domains = "clone-uuid"
        for name, value in [("RUNTIME", self.root)]:
            mocked = patch.object(vm, name, value)
            mocked.start()
            self.addCleanup(mocked.stop)
        for name, callback in [("base_records", lambda: self.bases),
                               ("definition", lambda _: self.xml),
                               ("virsh", self.virsh),
                               ("run", lambda *a, **kw: subprocess.CompletedProcess(a, 0, json.dumps(self.info), ""))]:
            mocked = patch.object(vm, name, side_effect=callback)
            mocked.start()
            self.addCleanup(mocked.stop)

    def virsh(self, *args, **kwargs):
        output = "shut off" if args[0] == "domstate" else self.domains
        return subprocess.CompletedProcess(args, 0, output, "")

    def test_independent_owned_clone_is_accepted(self):
        vm.guarded(self.record)

    def test_base_uuid_alias_is_rejected(self):
        self.record["uuid"] = "base-uuid"
        self.xml.find("uuid").text = "base-uuid"
        with self.assertRaisesRegex(ValueError, "base UUID"):
            vm.guarded(self.record)

    def test_replaced_domain_is_rejected(self):
        self.xml.find("uuid").text = "replacement"
        with self.assertRaisesRegex(ValueError, "ownership record"):
            vm.guarded(self.record)

    def test_base_hardlink_is_rejected(self):
        self.disk.unlink()
        self.disk.hardlink_to(self.base_disk)
        with self.assertRaisesRegex(ValueError, "aliases a base"):
            vm.guarded(self.record)

    def test_symlink_is_rejected(self):
        self.disk.unlink()
        self.disk.symlink_to(self.base_disk)
        with self.assertRaisesRegex(ValueError, "symbolic link"):
            vm.guarded(self.record)

    def test_backing_file_is_rejected(self):
        self.info["backing-filename"] = str(self.base_disk)
        with self.assertRaisesRegex(ValueError, "independent qcow2"):
            vm.guarded(self.record)

    def test_storage_shared_with_another_guest_is_rejected(self):
        self.domains += " other-uuid"
        with self.assertRaisesRegex(ValueError, "shared with another"):
            vm.guarded(self.record)

    def test_changed_storage_path_is_rejected(self):
        self.xml.find("./devices/disk/source").set("file", str(self.base_disk))
        with self.assertRaisesRegex(ValueError, "ownership record"):
            vm.guarded(self.record)


class NetworkDiagnosticsTests(unittest.TestCase):
    def test_diagnostics_are_read_only_and_saved(self):
        xml = ET.fromstring('<domain><devices><interface><source network="default"/></interface></devices></domain>')
        with tempfile.TemporaryDirectory(dir=vm.ROOT / "runtime") as directory, \
                patch.object(vm, "definition", return_value=xml), \
                patch.object(vm, "run", return_value=subprocess.CompletedProcess([], 0, "data", "")):
            vm.network_diagnostics({"uuid": "clone", "evidence": directory})
            rows = json.loads((Path(directory) / "network-diagnostics.json").read_text())
            actions = [row["command"][3] for row in rows if row["command"][0] == "virsh"]
            self.assertEqual(actions, ["domiflist", "domifaddr", "net-info", "net-dumpxml", "net-dhcp-leases"])
            self.assertTrue(all(row["returncode"] == 0 for row in rows))

    def test_success_does_not_collect_failure_diagnostics(self):
        with patch.object(vm, "wait_until", return_value="192.168.122.42"), \
                patch.object(vm, "network_diagnostics") as diagnostics:
            self.assertEqual(vm.wait_for_address({}), "192.168.122.42")
            diagnostics.assert_not_called()

    def test_timeout_collects_diagnostics_and_explains_host_network_boundary(self):
        record = {"uuid": "clone"}
        with patch.object(vm, "wait_until", side_effect=TimeoutError("lease")), \
                patch.object(vm, "network_diagnostics") as diagnostics:
            with self.assertRaisesRegex(TimeoutError, "No VPN/firewall settings were changed"):
                vm.wait_for_address(record)
            diagnostics.assert_called_once_with(record)

    def test_diagnostics_failure_does_not_mask_dhcp_timeout(self):
        with patch.object(vm, "wait_until", side_effect=TimeoutError("lease")), \
                patch.object(vm, "network_diagnostics", side_effect=OSError("unavailable")):
            with self.assertRaisesRegex(TimeoutError, "no discoverable DHCP lease"):
                vm.wait_for_address({})


if __name__ == "__main__":
    unittest.main()
