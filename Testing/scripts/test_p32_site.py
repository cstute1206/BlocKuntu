"""P3.2 browser routes stay deterministic and browser-specific."""
import importlib.util
from pathlib import Path
import threading
import unittest
from urllib.error import HTTPError
from urllib.request import urlopen


SITE = Path(__file__).resolve().parents[1] / 'fixtures/test-site/server.py'
spec = importlib.util.spec_from_file_location('layer3_test_site', SITE)
site = importlib.util.module_from_spec(spec)
spec.loader.exec_module(site)


class P32SiteTests(unittest.TestCase):
    def test_native_browser_matcher_and_spa_routes(self):
        server = site.FixtureServer(('127.0.0.1', 0), site.FixtureHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            base = f'http://127.0.0.1:{server.server_port}'
            for browser in ('firefox', 'chrome'):
                for route in ('exact/blocked', 'prefix/blocked/child', 'prefix/blockedish',
                              'path/blocked/child', 'path/blockedish', 'longrunning'):
                    path = f'/{browser}/{route}'
                    with urlopen(base + path) as response:
                        self.assertEqual(response.status, 200)
                        self.assertIn(f'BlocKuntu test route {path}', response.read().decode())
                with urlopen(base + f'/{browser}/contains?marker=blockuntu-marker-{browser}') as response:
                    self.assertIn(f'blockuntu-marker-{browser}', response.read().decode())
                with urlopen(base + f'/{browser}/spa') as response:
                    self.assertIn(f"history.pushState({{}}, '', '/{browser}/spa/target')",
                                  response.read().decode())
            with self.assertRaises(HTTPError) as missing:
                urlopen(base + '/firefox/unexpected')
            self.assertEqual(missing.exception.code, 404)
        finally:
            server.shutdown()
            thread.join(timeout=2)
            server.server_close()


if __name__ == '__main__':
    unittest.main()
