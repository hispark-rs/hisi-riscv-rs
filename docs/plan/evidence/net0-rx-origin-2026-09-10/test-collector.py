#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Reject tampered traffic evidence without accessing a board or build inputs."""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import sys

sys.dont_write_bytecode = True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--raw', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('collector', Path(__file__).with_name('collect-evidence.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    original = json.loads((args.raw / 'summary.json').read_bytes())
    controls, negatives = 0, 0
    with tempfile.TemporaryDirectory(prefix='net0-origin-evidence-') as root:
        directory = Path(root)
        captures = {f"run-{row['index']:02}.{role}.uart.log":
                    (args.raw / f"run-{row['index']:02}.{role}.uart.log").read_bytes()
                    for row in original['runs'] for role in ('target', 'peer')}
        for mutation in ('control', 'pass', 'payload', 'uart', 'rebuild', 'failed-round'):
            summary = copy.deepcopy(original)
            for name, data in captures.items():
                (directory / name).write_bytes(data)
            if mutation == 'pass':
                summary['runs'][0]['pass'] = False
            elif mutation == 'payload':
                summary['runs'][0]['payload']['received'] = 9
            elif mutation == 'uart':
                name = 'run-01.target.uart.log'
                (directory / name).write_bytes(captures[name] + b'changed\n')
            elif mutation == 'rebuild':
                summary['runs'][0]['rx_rebuild']['samples'][-1]['actual_normal'] = 3
            elif mutation == 'failed-round':
                summary['runs'][2]['pass'] = True
            (directory / 'summary.json').write_text(json.dumps(summary))
            try:
                result = module.check_traffic(directory)
            except ValueError:
                if mutation == 'control':
                    raise
                negatives += 1
            else:
                if mutation != 'control' or result['recorded_passes'] != 2:
                    raise ValueError('Unexpected acceptance of mutated evidence')
                controls += 1
    print(json.dumps({'control': controls, 'rejected_mutations': negatives}))


if __name__ == '__main__':
    main()
