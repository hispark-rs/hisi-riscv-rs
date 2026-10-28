#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Exercise the summary gate with synthetic reports, not hardware."""
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
import ws53_model_summary as summary

class SummaryGate(unittest.TestCase):
    def test_rejects_incomplete_and_hardware_results(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, count in {'hash': 198, 'km': 357, 'sdio-init': 10, 'sdio-transfer': 14,
                                'sdio-async': 15, 'symc': 154, 'sfc': 8}.items():
                folder = root / f'{name}-test'
                folder.mkdir()
                (folder / 'report.json').write_text(json.dumps(
                    {'passed': True, 'hardware_accessed': False, 'cases': count}))
            ipc = root / 'ipc-test'
            ipc.mkdir()
            (ipc / 'report.json').write_text(json.dumps({'passed': True, 'hardware_accessed': False,
                'polling_endpoint': True, 'rounds': [{'transmitted': 512, 'received': 512}] * 3}))
            with redirect_stdout(io.StringIO()):
                summary.run(root)
            data = json.loads((root / 'summary.json').read_text())
            self.assertEqual(data['models']['ipc']['frames_per_direction'], 1536)
            bad = root / 'hash-test/report.json'
            bad.write_text(json.dumps({'passed': True, 'hardware_accessed': False, 'cases': 197}))
            with self.assertRaises(ValueError):
                summary.run(root)
            bad.write_text(json.dumps({'passed': True, 'hardware_accessed': True, 'cases': 198}))
            with self.assertRaises(ValueError):
                summary.run(root)
            bad.write_text(json.dumps({'passed': True, 'hardware_accessed': False, 'cases': 198}))
            (ipc / 'report.json').write_text(json.dumps({'passed': True, 'hardware_accessed': False,
                'polling_endpoint': True, 'rounds': [{'transmitted': 512, 'received': 511}] * 3}))
            with self.assertRaises(ValueError):
                summary.run(root)

if __name__ == '__main__':
    unittest.main()
