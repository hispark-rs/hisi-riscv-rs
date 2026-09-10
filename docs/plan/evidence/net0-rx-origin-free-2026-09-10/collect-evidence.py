#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Bind the free-attempt census to complete captures, source bytes and ELF."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys
import unittest

FIELDS = ('bindings', 'failed', 'replaced', 'capacity', 'matched', 'unmatched',
          'closed', 'current', 'stale', 'occupied', 'peak')
PHASES = ('bootstrap', 'connected', 'payload', 'disconnected')
ORIGIN = re.compile(rb'RFDBG_NET0_RX_ORIGIN phase=(bootstrap|connected|payload|disconnected)' +
    b''.join(b' ' + key.encode() + rb'=0x([0-9a-f]{16})' for key in FIELDS) + rb' exhausted=([01])')
FREE = re.compile(rb'RFDBG_NET0_RX_FREE phase=(bootstrap|connected|payload|disconnected)'
                  rb' calls=0x([0-9a-f]{16}) retired=0x([0-9a-f]{16})')
sys.dont_write_bytecode = True


def sha(data):
    return hashlib.sha256(data).hexdigest()


def observations(data):
    samples, lines, free, errors = [], [], {}, []
    for line in data.splitlines():
        if match := ORIGIN.fullmatch(line):
            sample = dict(zip(FIELDS, (int(v, 16) for v in match.groups()[1:-1])))
            sample.update(phase=match[1].decode(), exhausted=match.groups()[-1] == b'1')
            samples.append(sample)
            lines.append(line.decode())
        elif match := FREE.fullmatch(line):
            phase = match[1].decode()
            if phase in free:
                errors.append('duplicate_free_phase')
            free[phase] = dict(free_calls=int(match[2], 16), retired=int(match[3], 16))
            lines.append(line.decode())
        elif line.startswith((b'RFDBG_NET0_RX_ORIGIN ', b'RFDBG_NET0_RX_FREE ')):
            errors.append('malformed_observation')
    if [s['phase'] for s in samples] != list(PHASES) or list(free) != list(PHASES):
        errors.append('missing_duplicate_or_reordered_phase')
    previous = None
    for sample in samples:
        if sample['phase'] not in free:
            continue
        sample.update(free[sample['phase']])
        if (sample['exhausted'] or sample['retired'] > sample['free_calls']
                or sample['bindings'] != sum(sample[k] for k in ('matched', 'replaced', 'retired', 'occupied'))
                or sample['matched'] != sum(sample[k] for k in ('closed', 'current', 'stale'))
                or not 0 <= sample['occupied'] <= sample['peak'] <= 16):
            errors.append('invalid_conservation')
        counters = [key for key in FIELDS if key != 'occupied'] + ['free_calls', 'retired']
        if previous and any(sample[key] < previous[key] for key in counters):
            errors.append('counter_regression')
        previous = sample
    if not samples or not samples[-1].get('free_calls') or not samples[-1]['bindings']:
        errors.append('observer_not_exercised')
    coverage = not errors and all(not s['failed'] and not s['capacity'] and not s['unmatched'] for s in samples)
    return dict(samples=samples, public_markers=lines, errors=errors, coverage_pass=coverage)


class Tests(unittest.TestCase):
    def fixture(self):
        lines = []
        values = (3, 0, 0, 0, 1, 0, 1, 0, 0, 1, 2)
        for phase in PHASES:
            line = 'RFDBG_NET0_RX_ORIGIN phase=' + phase
            line += ''.join(f' {key}=0x{value:016x}' for key, value in zip(FIELDS, values))
            lines += [line + ' exhausted=0', f'RFDBG_NET0_RX_FREE phase={phase} calls=0x0000000000000002 retired=0x0000000000000001']
        return ('\n'.join(lines) + '\n').encode()

    def test_complete_census_and_mutations(self):
        good = self.fixture()
        self.assertTrue(observations(good)['coverage_pass'])
        for old, new in [(b'bindings=0x0000000000000003', b'bindings=0x0000000000000004'),
                         (b'exhausted=0', b'exhausted=1'),
                         (b'capacity=0x0000000000000000', b'capacity=0x0000000000000001'),
                         (b'unmatched=0x0000000000000000', b'unmatched=0x0000000000000001'),
                         (b'retired=0x0000000000000001', b'retired=0x0000000000000003')]:
            self.assertFalse(observations(good.replace(old, new, 1))['coverage_pass'])
        self.assertFalse(observations(b'\n'.join(good.splitlines()[:-1]))['coverage_pass'])
        self.assertFalse(observations(good + good.splitlines()[1] + b'\n')['coverage_pass'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path)
    parser.add_argument('--raw', type=Path)
    parser.add_argument('--build', type=Path)
    parser.add_argument('--source')
    parser.add_argument('--ap-elf', type=Path)
    parser.add_argument('--capture-tool', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--test', action='store_true')
    args = parser.parse_args()
    if args.test:
        if not unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests)).wasSuccessful():
            raise SystemExit(1)
    if args.raw is None:
        return
    old_path = args.root / 'docs/plan/evidence/net0-rx-rebuild-2026-09-10/collect-evidence.py'
    spec = importlib.util.spec_from_file_location('rebuild', old_path)
    rebuild = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(rebuild)
    elf = sha((args.build / 'sta.elf').read_bytes())
    ap_elf = sha(args.ap_elf.read_bytes())
    if ap_elf != 'd54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091':
        raise ValueError('fixed AP changed')
    capture = args.root / 'docs/plan/evidence/net0-rx-rebuild-2026-09-10/capture-tool.py'
    if capture.read_bytes() != args.capture_tool.read_bytes():
        raise ValueError('capture tool differs from the retained contract')
    traffic = rebuild.stop.collect(args.raw, 3, elf, 'observation-only-preflight')
    if (not 1 <= len(traffic['runs']) <= 3
            or (len(traffic['runs']) < 3 and traffic['runs'][-1]['pass'])):
        raise ValueError('preserve the requested rounds or the stop-on-first-failure receipt')
    for row in traffic['runs']:
        data = (args.raw / f"run-{row['index']:02}.target.uart.log").read_bytes()
        row['origin'] = observations(data)
        samples = [dict(zip(rebuild.FIELDS, (int(v, 16) for v in match.groups())))
                   for line in data.splitlines() if (match := rebuild.RX.fullmatch(line))]
        if samples != row['rx_rebuild']['samples'] or rebuild.valid(samples) != row['rx_rebuild']['pass']:
            raise ValueError('rebuild receipt differs from UART')
    inputs = []
    backend = args.root / 'crates/chips/ws63/hisi-rf-ws63'
    names = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', '-z', args.source, '--',
        'src', 'examples', 'build.rs', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo'], cwd=backend).decode().split('\0')
    for name in sorted(filter(None, names)):
        data = subprocess.check_output(['git', 'show', args.source + ':' + name], cwd=backend)
        if data != (args.build / name).read_bytes():
            raise ValueError('source/build mismatch: ' + name)
        inputs.append(dict(path=name, sha256=sha(data)))
    physical = args.raw / 'target/riscv32imfc-unknown-none-elf/release'
    reports = {}
    for name in ('origin-link.json', 'storage.json'):
        data = (args.build / name).read_bytes()
        report = json.loads(data)
        if report['elf_sha256'] != elf or report['status'] != 'pass':
            raise ValueError('static report differs from the HIL ELF')
        reports[name] = sha(data)
    result = dict(schema='net0-rx-origin-free-hil/v1', source_commit=args.source,
        elf_sha256=elf, build_inputs=inputs, collector_sha256=sha(Path(__file__).read_bytes()),
        ap_elf_sha256=ap_elf, image_sha256=sha((physical / 'sta.img').read_bytes()),
        flash_plan_sha256=sha((physical / 'sta.plan.json').read_bytes()),
        static_reports=reports,
        capture_tool=dict(path='../net0-rx-rebuild-2026-09-10/capture-tool.py', sha256=sha(capture.read_bytes())),
        public_paired_config=dict(path='examples/ws63/hil_wifi_config.rs',
            sha256=sha((args.root / 'examples/ws63/hil_wifi_config.rs').read_bytes())),
        build_features=['wpa2-personal', 'standard-l2-rx-origin-experiment',
            'standard-l2-rx-stop-experiment', 'smoltcp', 'incremental-connect-profile',
            'bootstrap-stage-diag', 'firmware-example'],
        traffic=traffic, coverage_passes=sum(row['origin']['coverage_pass'] for row in traffic['runs']),
        boundary='Free-attempt observation only; no allocation lifetime, DMA fence, generation propagation or reconnect acceptance')
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(dict(traffic_passes=traffic['recorded_passes'], coverage_passes=result['coverage_passes'],
        udp_sent=traffic['udp_sent'], udp_received=traffic['udp_received'])))


if __name__ == '__main__':
    main()
