"""A failed supervisor must retain completed observations without a pass."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location('checkpoint_guest',
    Path(__file__).parents[1] / 'fixtures/layer3/guest.py')
guest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guest)


class ProcessCheckpointTests(unittest.TestCase):
    def test_ordered_partial_results_are_readable(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'process-results.json'
            rows = [{'id': 'VM-APP-001', 'status': 'pass', 'seconds': 12.4},
                    {'id': 'VM-APP-002', 'status': 'fail', 'seconds': 1.2}]
            path.write_text(json.dumps(rows))
            self.assertEqual(guest.process_rows(path), rows)

    def test_misattributed_or_duplicate_checkpoint_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'process-results.json'
            for rows in ([{'id': 'VM-AAL-004', 'status': 'pass', 'seconds': 1}],
                         [{'id': 'VM-APP-001', 'status': 'pass', 'seconds': 1}] * 2,
                         [{'id': 'VM-APP-001', 'status': 'planned', 'seconds': 1}]):
                path.write_text(json.dumps(rows))
                with self.subTest(rows=rows), self.assertRaises(ValueError):
                    guest.process_rows(path)


if __name__ == '__main__':
    unittest.main()
