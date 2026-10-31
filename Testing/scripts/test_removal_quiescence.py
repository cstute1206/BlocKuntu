"""Run the actual removal guard/scriptlets against an isolated service manager."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOKEN = 'a' * 64


class RemovalQuiescenceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.work = self.root / 'transaction'
        self.runtime = self.root / 'systemd'
        self.lease = self.root / 'run-data/package-removal-lease'
        self.lease.parent.mkdir()
        self.lease.write_text(f'{TOKEN} {int(time.time()) + 120}\n')
        self.guard, self.helper = self.root / 'guard.sh', self.root / 'prepare.sh'
        self.config, self.data = self.root / 'config-data', self.root / 'state-data'
        for path in (self.config, self.data):
            path.mkdir()
            (path / 'policy').write_text('must survive a failed preparation')
        self.state = self.root / 'manager.json'
        self.state.write_text(json.dumps({'active': True, 'calls': [], 'fault': '', 'blocked_starts': 0}))
        mapping = {
            '/run/blockuntu-package-removal': str(self.work),
            '/run/systemd/system': str(self.runtime),
            '/run/blockuntu/package-removal-lease': str(self.lease),
            '/usr/lib/blockuntu/check-package-removal-lease.sh': str(self.guard),
            '/usr/lib/blockuntu/prepare-package-removal.sh': str(self.helper),
            '/usr/lib/blockuntu/remove-user-native-hosts.sh': '/bin/true',
            '/etc/blockuntu': str(self.config), '/var/lib/blockuntu': str(self.data),
            '/run/blockuntu': str(self.lease.parent),
        }
        def rewritten(path):
            text = (ROOT / path).read_text()
            for old in sorted(mapping, key=len, reverse=True):
                text = text.replace(old, mapping[old])
            return text.replace(' + 30 ))', ' + 1 ))')
        for source, target in [('scripts/check-package-removal-lease.sh', self.guard),
                               ('scripts/prepare-package-removal.sh', self.helper),
                               ('packaging/arch/blockuntu.install', self.root / 'install.sh')]:
            target.write_text(rewritten(source))
            target.chmod(0o755)
        binary = self.root / 'bin'
        binary.mkdir()
        systemctl = binary / 'systemctl'
        systemctl.write_text('''#!/usr/bin/env python3
import json,os,sys
from pathlib import Path
p=Path(os.environ['TEST_MANAGER_STATE']);d=json.loads(p.read_text())
a=[x for x in sys.argv[1:] if x!='--no-block'];d['calls'].append(a)
command=a[0];root=p.parent;work=root/'transaction';runtime=root/'systemd'
code=0
if command=='daemon-reload' and d['fault']=='reload':
 d['fault']='';code=1;print('reload denied',file=sys.stderr)
elif command=='stop':
 if d['fault']=='stop': code=1;print('stop denied',file=sys.stderr)
 else:
  d['active']=d['fault']=='survivor'
  text=(runtime/'blockuntu.service.d/99-blockuntu-removal.conf').read_text()
  if 'RuntimeDirectoryPreserve=yes' not in text:
   (root/'run-data/package-removal-lease').unlink(missing_ok=True)
  if (work/'preparing').exists() and 'ConditionPathExists=!' in text:
   d['blocked_starts']+=1
  else:d['active']=True
elif command=='start': d['active']=True
elif command=='show':
 unit=a[1];pid=999 if d['active'] or d['fault']=='pid' else 0
 print('Id='+unit+'\\nActiveState='+('active' if d['active'] else 'inactive')+
       '\\nMainPID='+str(pid)+'\\nControlPID=0')
p.write_text(json.dumps(d));sys.exit(code)
''')
        systemctl.chmod(0o755)
        self.env = {**os.environ, 'PATH': str(binary) + os.pathsep + os.environ['PATH'],
                    'TEST_MANAGER_STATE': str(self.state), 'BLOCKUNTU_PACKAGE_REMOVAL_LEASE': TOKEN}

    def run_guard(self):
        return subprocess.run([str(self.guard), '--pacman-prepare'], env=self.env,
                              text=True, capture_output=True, timeout=10)

    def fault(self, fault):
        state = json.loads(self.state.read_text())
        state['fault'] = fault
        self.state.write_text(json.dumps(state))

    def assert_preserved(self):
        self.assertTrue((self.config / 'policy').exists())
        self.assertTrue((self.data / 'policy').exists())
        self.assertTrue(self.lease.exists())
        self.assertTrue(json.loads(self.state.read_text())['active'])
        self.assertFalse(self.work.exists())
        self.assertEqual(list(self.runtime.glob('*/99-blockuntu-removal.conf')), [])

    def test_rejected_authorization_has_no_systemd_side_effects(self):
        self.env['BLOCKUNTU_PACKAGE_REMOVAL_LEASE'] = 'b' * 64
        self.assertNotEqual(self.run_guard().returncode, 0)
        self.assertEqual(json.loads(self.state.read_text())['calls'], [])
        self.assert_preserved()

    def test_stop_error_aborts_before_removal_and_restores_protection(self):
        self.fault('stop')
        result = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('stop denied', result.stderr)
        self.assert_preserved()

    def test_reload_error_aborts_and_restores_protection(self):
        self.fault('reload')
        self.assertNotEqual(self.run_guard().returncode, 0)
        self.assert_preserved()

    def test_successful_stop_command_with_survivors_is_not_sufficient(self):
        for fault in ('survivor', 'pid'):
            with self.subTest(fault=fault):
                self.fault(fault)
                self.assertNotEqual(self.run_guard().returncode, 0)
                self.assert_preserved()

    def test_guard_keeps_lease_and_restart_barrier_until_actual_removal(self):
        original = self.lease.read_bytes()
        result = self.run_guard()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.lease.read_bytes(), original)
        self.assertTrue((self.work / 'preparing').exists())
        state = json.loads(self.state.read_text())
        self.assertFalse(state['active'])
        self.assertGreater(state['blocked_starts'], 0)
        # Repeated authorization checks must not consume the transaction lease.
        self.assertEqual(self.run_guard().returncode, 0)
        script = self.root / 'install.sh'
        command = '. "$1"; remove_hosts_block() { :; }; remove_browser_policies() { :; }; pre_remove'
        before = subprocess.run(['sh', '-c', command, 'test', str(script)], env=self.env,
                                text=True, capture_output=True, timeout=10)
        self.assertEqual(before.returncode, 0, before.stderr)
        self.assertFalse(self.lease.exists())
        # Pacman deletes the installed helpers before executing post_remove.
        self.guard.unlink()
        self.helper.unlink()
        after = subprocess.run(['sh', '-c', '. "$1"; post_remove', 'test', str(script)],
                               env=self.env, text=True, capture_output=True, timeout=10)
        self.assertEqual(after.returncode, 0, after.stderr)
        self.assertFalse(self.config.exists())
        self.assertFalse(self.data.exists())
        self.assertFalse(self.lease.parent.exists())
        self.assertFalse(self.work.exists())
        self.assertFalse(json.loads(self.state.read_text())['active'])

    def test_post_remove_retains_policy_if_a_service_is_still_running(self):
        self.assertEqual(self.run_guard().returncode, 0)
        state = json.loads(self.state.read_text())
        state['active'] = True
        self.state.write_text(json.dumps(state))
        result = subprocess.run(['sh', '-c', '. "$1"; post_remove', 'test', str(self.root / 'install.sh')],
                                env=self.env, text=True, capture_output=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue((self.config / 'policy').exists())
        self.assertTrue((self.data / 'policy').exists())
