"""Runtime regressions found while porting the native destructive journeys."""
import importlib.util
import datetime as dt
import json
from pathlib import Path
import sqlite3
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from unittest.mock import Mock, patch
from layer4_uninstall_contract import handoff_heartbeats, installed_packages_preserved

HERE = Path(__file__).resolve().parent

def fixture(name, relative):
    spec = importlib.util.spec_from_file_location(name, HERE.parent / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

class NativeFollowups(unittest.TestCase):
    def test_refusal_requires_every_installed_package_unchanged(self):
        before = {'guest': 'fedora', 'returncode': 0, 'packages': ['blockuntu 0:0.2.0-1.x86_64', 'gui-library 0:1-2.x86_64']}
        self.assertTrue(installed_packages_preserved(before, dict(before)))
        self.assertFalse(installed_packages_preserved(before, {**before, 'packages': before['packages'][:1]}))
        self.assertFalse(installed_packages_preserved(before, {**before, 'packages': ['blockuntu 0:0.2.0-1.x86_64', 'gui-library 0:2-2.x86_64']}))
        for invalid in [None, {**before, 'returncode': 1}, {**before, 'packages': []},
                        {**before, 'packages': before['packages'] * 2}]:
            with self.assertRaises(ValueError): installed_packages_preserved(before, invalid)

    def test_schedule_uses_captured_interval_even_after_checkpoint_crosses_boundary(self):
        guest = type(sys)('guest'); guest.WORK = Path('/unused')
        for name in ('host_action', 'rpc', 'run', 'wait'): setattr(guest, name, Mock())
        with patch.dict(sys.modules, {'guest': guest}):
            hosts = fixture('followup_schedule_hosts', 'fixtures/layer4/hosts_repair.py')
        start = dt.datetime.fromisoformat('2026-10-10T22:38:00+02:00')
        end = start + dt.timedelta(minutes=1)
        def stage(name, began, finished):
            return {'stage': name, 'observation_started_at': began.isoformat(), 'at': finished.isoformat()}
        # Evidence collection happens after midnight or the next schedule boundary.
        with patch.object(hosts.dt, 'datetime', wraps=dt.datetime) as clock:
            clock.now.return_value = end + dt.timedelta(hours=1)
            for observation in [stage('inactive', start - dt.timedelta(seconds=10), start - dt.timedelta(seconds=6)),
                                stage('active', start, end - dt.timedelta(seconds=1)),
                                stage('ended', end, end + dt.timedelta(seconds=4))]:
                hosts.schedule_observation_in_window(observation, start, end)
            clock.now.assert_not_called()
        for observation in [stage('inactive', start - dt.timedelta(seconds=1), start),
                            stage('active', start - dt.timedelta(seconds=1), start),
                            stage('active', start, end),
                            stage('ended', end - dt.timedelta(seconds=1), end),
                            stage('inactive', start, start - dt.timedelta(seconds=1))]:
            with self.assertRaises(AssertionError):
                hosts.schedule_observation_in_window(observation, start, end)

    def test_handoff_uses_actual_transition_and_rejects_old_or_other_session_heartbeats(self):
        sample = {'state': 'uninstalling', 'at': '2026-10-09T19:47:39.490767+00:00',
                  'handoff': {'mode': 'uninstalling', 'started_at': '2026-10-09T19:47:38Z'},
                  'browsers': {'chromium': {'current_session_heartbeat': True,
                                           'last_seen_at': '2026-10-09T19:47:39.490648Z'}}}
        self.assertEqual(handoff_heartbeats([sample], ['chromium']), {'chromium': True})
        sample['browsers']['chromium']['last_seen_at'] = '2026-10-09T19:47:37.99Z'
        self.assertEqual(handoff_heartbeats([sample], ['chromium']), {'chromium': False})
        sample['browsers']['chromium'].update(last_seen_at='2026-10-09T19:47:40Z', current_session_heartbeat=False)
        self.assertEqual(handoff_heartbeats([sample], ['chromium']), {'chromium': False})
        sample.pop('handoff')
        with self.assertRaisesRegex(ValueError, 'transition witness'):
            handoff_heartbeats([sample], ['chromium'])

    def test_daemon_transition_read_is_read_only_and_never_creates_missing_database(self):
        ops = fixture('followup_uninstall_ops', 'fixtures/layer4/uninstall_ops.py')
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'state.sqlite3'
            self.assertIsNone(ops.browser_handoff(path))
            self.assertFalse(path.exists())
            with sqlite3.connect(path) as connection:
                connection.execute('CREATE TABLE service_state(key TEXT,value TEXT,updated_at TEXT)')
                connection.execute("INSERT INTO service_state VALUES('browser_extension_mode','uninstalling','2026-10-09T19:47:38Z')")
            before = path.read_bytes()
            self.assertEqual(ops.browser_handoff(path), {'mode': 'uninstalling', 'started_at': '2026-10-09T19:47:38Z'})
            self.assertEqual(path.read_bytes(), before)

    def test_real_http_fixture_serves_native_browser_routes_and_rejects_unknown_routes(self):
        site = fixture('followup_site', 'fixtures/test-site/server.py')
        server = site.FixtureServer(('127.0.0.1', 0), site.FixtureHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        server.request_log = None
        try:
            base = 'http://127.0.0.1:' + str(server.server_port)
            for browser in ('firefox', 'chrome', 'chromium'):
                route = '/' + browser + '/exact/blocked'
                with urllib.request.urlopen(base + route, timeout=3) as response:
                    self.assertEqual(response.status, 200)
                    self.assertIn('BlocKuntu test route ' + route, response.read().decode())
            with self.assertRaises(urllib.error.HTTPError) as error:
                urllib.request.urlopen(base + '/unknown/exact/blocked', timeout=3)
            self.assertEqual(error.exception.code, 404)
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=3)

    def test_browser_recovery_waits_for_actual_native_rule_and_url_after_reused_title(self):
        guest = type(sys)('guest'); guest.WORK = Path('/unused')
        for name in ('configure_desktop', 'host_action', 'rpc', 'run', 'wait'): setattr(guest, name, Mock())
        browser_api = type(sys)('browsers'); browser_api.APPS = {'firefox': 'Firefox', 'chromium': 'Chromium'}
        with patch.dict(sys.modules, {'guest': guest, 'browsers': browser_api,
                                    'accessibility': type(sys)('accessibility'),
                                    'ui_controls': type(sys)('ui_controls')}):
            page = fixture('followup_uninstall_page', 'fixtures/layer4/uninstall.py')
            with patch.dict(sys.modules, {'uninstall': page}):
                recovery = fixture('followup_browser_recovery', 'fixtures/layer4/browser_recovery.py')
        for browser in ('firefox', 'chromium'):
            url = 'http://web.blockuntu.test:18080/' + browser + '/exact/blocked'
            stale = 'Blocked by BlocKuntu\nDaemon heartbeat missing\n' + url
            wrong = 'Blocked by BlocKuntu\nP41 exact chrome\nhttp://web.blockuntu.test:18080/chrome/exact/blocked'
            named = 'Blocked by BlocKuntu\nP41 exact ' + browser + '\n' + url
            page.visible_document_text = Mock(side_effect=[stale, wrong, named])
            def bounded(ready, seconds):
                self.assertEqual(seconds, 45); self.assertIsNone(ready()); self.assertIsNone(ready()); return ready()
            page.wait = bounded
            self.assertEqual(recovery.wait_recovered_block(browser), named)

    def test_hosts_wait_requires_real_dns_and_http_denial_with_accessible_controls(self):
        guest = type(sys)('guest'); guest.WORK = Path('/unused')
        for name in ('host_action', 'rpc', 'run', 'wait'): setattr(guest, name, Mock())
        with patch.dict(sys.modules, {'guest': guest}):
            hosts = fixture('followup_hosts', 'fixtures/layer4/hosts_repair.py')
        stale = {'hard_resolution_blocked': False, 'hard_http_blocked': False, 'control_http_accessible': True}
        blocked = dict(stale, hard_resolution_blocked=True, hard_http_blocked=True)
        bad_control = dict(blocked, control_http_accessible=False)
        hosts.connectivity = Mock(side_effect=[(stale, {'stage': 'stale'}), (bad_control, {'stage': 'control-down'}), (blocked, {'stage': 'settled'})])
        def bounded(ready, seconds):
            self.assertEqual(seconds, 30)
            self.assertIsNone(ready()); self.assertIsNone(ready()); return ready()
        hosts.wait = bounded; hosts.save = Mock()
        self.assertEqual(hosts.wait_connectivity(['blocked.test']), (blocked, {'stage': 'settled'}))
        self.assertEqual(len(hosts.save.call_args.args[1]), 3)
        hosts.connectivity = Mock(return_value=(stale, {}))
        def timeout(ready, seconds):
            self.assertIsNone(ready()); raise TimeoutError('normal DNS stayed stale')
        hosts.wait = timeout
        with self.assertRaisesRegex(TimeoutError, 'stayed stale'):
            hosts.wait_connectivity(['blocked.test'])

if __name__ == '__main__': unittest.main()
