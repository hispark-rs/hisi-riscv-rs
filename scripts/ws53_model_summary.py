#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Verify and summarize the WS53 synthetic model reports."""
import argparse
import hashlib
import json
from pathlib import Path

def run(root):
    expected = {'hash': 198, 'km': 357, 'sdio-init': 10, 'sdio-transfer': 14,
                'sdio-async': 15, 'symc': 154, 'sfc': 8}
    rows = {}
    for name, count in expected.items():
        path = root / f'{name}-test/report.json'
        content = path.read_bytes()
        report = json.loads(content)
        if (report.get('passed') is not True or report.get('hardware_accessed') is not False or
                type(report.get('cases')) is not int or report['cases'] != count):
            raise ValueError(f'invalid model report: {name}')
        rows[name] = {'cases': report['cases'], 'report_sha256': hashlib.sha256(content).hexdigest()}
    ipc_path = root / 'ipc-test/report.json'
    ipc_content = ipc_path.read_bytes()
    ipc = json.loads(ipc_content)
    rounds = ipc['rounds']
    if (ipc.get('passed') is not True or ipc.get('hardware_accessed') is not False or
            ipc.get('polling_endpoint') is not True or len(rounds) != 3 or
            any(r['transmitted'] != 512 or r['received'] != 512 for r in rounds)):
        raise ValueError('invalid IPC endpoint report')
    rows['ipc'] = {'rounds': len(rounds), 'frames_per_direction': 1536,
                   'report_sha256': hashlib.sha256(ipc_content).hexdigest()}
    result = {'passed': True, 'hardware_accessed': False, 'models': rows}
    (root / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    run(parser.parse_args().output.resolve())
