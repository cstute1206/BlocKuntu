#!/usr/bin/env python3
"""Exercise the real shell guards with isolated lease files; no package removal."""
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOKEN = 'a' * 64


class RemovalGuardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.lease = Path(self.temp.name) / 'lease'
        self.guard = Path(self.temp.name) / 'guard.sh'
        self.guard.write_text((ROOT / 'scripts/check-package-removal-lease.sh').read_text().replace(
            '/run/blockuntu/package-removal-lease', str(self.lease)))

    def run_guard(self, token=TOKEN, args=()):
        return subprocess.run(['sh', str(self.guard), *args],
                              env={**os.environ, 'BLOCKUNTU_PACKAGE_REMOVAL_LEASE': token},
                              capture_output=True)

    def write_lease(self, token=TOKEN, expiry=None):
        self.lease.write_text(f'{token} {expiry if expiry is not None else int(time.time()) + 120}\n')

    def test_valid_checks_are_repeatable_and_do_not_consume_lease(self):
        self.write_lease()
        before = self.lease.read_bytes()
        for args in [(), ('--dnf-action', 'E'), ()]:
            result = self.run_guard(args=args)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(self.lease.read_bytes(), before)

    def test_missing_mismatched_empty_and_malformed_token_fail_closed(self):
        self.assertNotEqual(self.run_guard().returncode, 0)
        self.write_lease()
        for token in ['', 'b' * 64, 'a' * 63, 'g' * 64]:
            with self.subTest(token=token):
                self.assertNotEqual(self.run_guard(token).returncode, 0)
                self.assertTrue(self.lease.exists())

    def test_expired_and_malformed_lease_fail_without_consuming(self):
        for contents in [f'{TOKEN} {int(time.time()) - 1}\n', f'{TOKEN} never\n',
                         f'{TOKEN} 999999999999999999999\n', f'{TOKEN}\n',
                         f'{TOKEN} {int(time.time()) + 120} extra\n', '']:
            with self.subTest(contents=contents):
                self.lease.write_text(contents)
                self.assertNotEqual(self.run_guard().returncode, 0)
                self.assertEqual(self.lease.read_text(), contents)

    def test_symlink_lease_is_rejected(self):
        target = self.lease.with_name('real-lease')
        target.write_text(f'{TOKEN} {int(time.time()) + 120}\n')
        self.lease.symlink_to(target)
        self.assertNotEqual(self.run_guard().returncode, 0)

    def test_dnf_replacement_is_unaffected_and_unknown_action_is_rejected(self):
        self.assertEqual(self.run_guard(token='', args=('--dnf-action', 'O')).returncode, 0)
        for args in [('--dnf-action', '?'), ('--dnf-action', 'E'), ('--dnf-action',), ('--anything',)]:
            self.assertNotEqual(self.run_guard(args=args).returncode, 0)

    def test_arch_scriptlet_refuses_without_fallthrough_and_consumes_once(self):
        script = Path(self.temp.name) / 'install.sh'
        script.write_text((ROOT / 'packaging/arch/blockuntu.install').read_text().replace(
            '/run/blockuntu/package-removal-lease', str(self.lease)))
        def authorize(token):
            return subprocess.run(['sh', '-c', '. "$1"; authorize_settings_uninstall', 'test', str(script)],
                                  env={**os.environ, 'BLOCKUNTU_PACKAGE_REMOVAL_LEASE': token}, capture_output=True)
        self.assertNotEqual(authorize('').returncode, 0)
        for token, expiry, supplied in [(TOKEN, int(time.time()) - 1, TOKEN),
                                        (TOKEN, 'bad', TOKEN), (TOKEN, int(time.time()) + 120, 'b' * 64)]:
            self.write_lease(token, expiry)
            self.assertNotEqual(authorize(supplied).returncode, 0)
            self.assertTrue(self.lease.exists())
        self.write_lease()
        self.assertEqual(authorize(TOKEN).returncode, 0)
        self.assertFalse(self.lease.exists())
        self.assertNotEqual(authorize(TOKEN).returncode, 0)


if __name__ == '__main__':
    unittest.main()
