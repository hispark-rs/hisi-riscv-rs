#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Check that a passing virtual SPI trace rejects damaged wire evidence."""
import argparse
import json
from pathlib import Path
import tempfile
import ws53_spi_wire_trace as trace


def run(report_path, trace_path):
    report = json.loads(report_path.read_text())
    if not report['passed'] or report['hardware_accessed']:
        raise ValueError('requires a passing model-only report')
    observation = report['observation']
    lines = [list(map(int, row.split(','))) for row in trace_path.read_text().splitlines()]
    mutations = {
        'missing-clock-pad': [row for row in lines if row[1] != 8],
        'forced-inactive-cs': [[*row[:2], 1, *row[3:]] if row[1] == 36 else row for row in lines],
        'inverted-mosi': [[*row[:2], 1-row[2], *row[3:]] if row[1] == 9 else row for row in lines],
        'floating-miso': [[*row[:4], 1, row[5]] if row[1] == 12 else row for row in lines],
    }
    with tempfile.TemporaryDirectory(prefix='ws53-spi-negative-') as directory:
        for name, rows in mutations.items():
            path = Path(directory)/f'{name}.csv'
            path.write_text(''.join(','.join(map(str, row))+'\n' for row in rows))
            try:
                trace.analyze(path, observation)
            except ValueError:
                continue
            raise ValueError(f'damaged SPI trace accepted: {name}')
    print(json.dumps(dict(passed=True, rejected=list(mutations), hardware_accessed=False)))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--trace', type=Path, required=True)
    args = parser.parse_args()
    run(args.report, args.trace)
