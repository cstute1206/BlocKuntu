"""RPC waits honor their budgets without replaying mutations or hiding expiry."""
from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import socket
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from test_layer3_dependencies import load_fixture

spec = importlib.util.spec_from_file_location('rpc_guest_fixture',
    Path(__file__).parents[1] / 'fixtures/layer3/guest.py')
guest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guest)


@contextmanager
def delayed_peer(delay):
    requests = []
    with tempfile.TemporaryDirectory() as temporary, socket.socket(socket.AF_UNIX) as listener:
        path = str(Path(temporary) / 'rpc.sock')
        listener.bind(path)
        listener.listen(1)
        listener.settimeout(2)
        def serve():
            connection, _ = listener.accept()
            with connection:
                connection.settimeout(2)
                payload = b''
                while chunk := connection.recv(65536):
                    payload += chunk
                requests.append(json.loads(payload))
                threading.Event().wait(delay)
                try:
                    connection.sendall(b'{"result":{"decision":"allow"}}')
                except BrokenPipeError:
                    pass
        thread = threading.Thread(target=serve, daemon=True)
        thread.start()
        try:
            yield path, requests
        finally:
            thread.join(3)
            if thread.is_alive():
                raise AssertionError('RPC fixture peer did not finish')


class RpcBudgetTests(unittest.TestCase):
    def test_explicit_budget_accepts_a_delayed_real_unix_response(self):
        with delayed_peer(0.05) as (path, requests), patch.object(guest, 'RPC_SOCKET', path):
            self.assertEqual(guest.rpc('evaluate_url', {'probe': True}, timeout=1), {'decision': 'allow'})
        self.assertEqual(len(requests), 1)

    def test_timeout_identifies_method_without_replaying_a_mutation_or_leaking_params(self):
        with delayed_peer(0.1) as (path, requests), patch.object(guest, 'RPC_SOCKET', path):
            with self.assertRaises(TimeoutError) as caught:
                guest.rpc('start_detox', {'reason': 'private fixture value'}, timeout=0.02)
        self.assertIn('RPC start_detox timed out', str(caught.exception))
        self.assertNotIn('private fixture value', str(caught.exception))
        self.assertEqual(len(requests), 1)

    def test_expiry_probe_uses_plan_rpc_budget_and_never_extends_overall_deadline(self):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            (work / 'smoke-plan.json').write_text(json.dumps({'deadlines_seconds': {'rpc': 60}}))
            browsers = load_fixture('browsers', work)
            elapsed = 0
            budgets = []
            def advance(seconds):
                nonlocal elapsed
                elapsed += seconds
            def blocked(method, params, *, timeout):
                self.assertEqual(method, 'evaluate_url')
                self.assertIs(params['probe'], True)
                self.assertGreater(timeout, 0)
                self.assertLessEqual(timeout, 225 - elapsed)
                budgets.append(timeout)
                advance(timeout)
                return {'decision': 'block'}
            with patch.object(browsers, 'time', SimpleNamespace(monotonic=lambda: elapsed, sleep=advance)), \
                    patch.object(browsers, 'rpc', side_effect=blocked):
                with self.assertRaisesRegex(TimeoutError, 'did not expire within 225s'):
                    browsers.wait_for_allowlist_expiry()
            self.assertEqual(elapsed, 225)
            self.assertEqual(max(budgets), 60)
            self.assertLess(budgets[-1], 60)


if __name__ == '__main__':
    unittest.main()
