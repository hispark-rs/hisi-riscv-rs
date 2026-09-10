#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Preserve failed native-origin coverage separately from one-shot traffic."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys

HERE = Path(__file__).resolve().parent
SOURCE = '70efda369aa9b8914b6d05659783a8a24b3d9e63'
ELF = '9f8918bf623781ace088000ac03810b065c9fae12bb6616c329762d5999076a4'
AP = 'd54e79d07df9ea860da1bb5858e0ff96c2c2a1964e1bf1ce4e9c1c360665b091'
FIELDS = ('bindings', 'failed', 'replaced', 'capacity', 'matched', 'unmatched', 'closed',
          'current', 'stale', 'occupied', 'peak')
PATTERN = (rb'RFDBG_NET0_RX_ORIGIN phase=(bootstrap|connected|payload|disconnected)' +
           b''.join(b' ' + field.encode() + rb'=0x([0-9a-fA-F]{16})' for field in FIELDS) +
           rb' exhausted=([01])\r?')
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('rebuild', HERE.parent / 'net0-rx-rebuild-2026-09-10/collect-evidence.py')
rebuild = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rebuild)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def check_traffic(directory):
    case = rebuild.stop.collect(directory, 3, ELF, 'failed-origin-preflight')
    if len(case['runs']) != 3:
        raise ValueError('The fixed preflight must preserve all three rounds')
    for row in case['runs']:
        data = (directory / f"run-{row['index']:02}.target.uart.log").read_bytes()
        samples = [dict(zip(rebuild.FIELDS, (int(value, 16) for value in match.groups())))
                   for line in data.splitlines() if (match := rebuild.RX.fullmatch(line))]
        if (samples != row['rx_rebuild']['samples']
                or rebuild.valid(samples) != row['rx_rebuild']['pass']
                or (row['pass'] and not rebuild.valid(samples))):
            raise ValueError('Stored rebuild result differs from complete UART receipts')
    return case


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--raw', type=Path, required=True)
    parser.add_argument('--backend', type=Path, required=True)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--ap-elf', type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        parser.error('Evidence assertions must remain enabled')
    assert sha((args.build / 'sta.elf').read_bytes()) == ELF
    assert sha(args.ap_elf.read_bytes()) == AP
    names = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', '-z', SOURCE, '--',
        'src', 'examples', 'build.rs', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo'], cwd=args.backend).decode().split('\0')
    inputs = []
    for name in sorted(filter(None, names)):
        data = subprocess.check_output(['git', 'show', SOURCE + ':' + name], cwd=args.backend)
        assert data == (args.build / name).read_bytes(), 'build input differs: ' + name
        inputs.append({'path': name, 'sha256': sha(data)})
    original = (args.raw / 'summary.json').read_bytes()
    raw = json.loads(original)
    traffic = check_traffic(args.raw)
    assert raw['elf_sha256'] == ELF and len(raw['runs']) == 3
    assert raw['flash']['exit_code'] == 0 and raw['flash']['verify_requested']
    assert raw['flash']['speed_khz'] == 3000
    cases = []
    for row in traffic['runs']:
        captures = []
        for role in ('target', 'peer'):
            data = (args.raw / f"run-{row['index']:02}.{role}.uart.log").read_bytes()
            record = next(item for item in row['captures'] if item['role'] == role)
            assert sha(data) == record['sha256'] and len(data) == record['bytes']
            captures.append(record)
            if role == 'target':
                target = data
        samples, lines = [], []
        for line in target.splitlines():
            match = re.fullmatch(PATTERN, line)
            if not match:
                continue
            sample = dict(zip(FIELDS, (int(value, 16) for value in match.groups()[1:-1])))
            sample['phase'], sample['exhausted'] = match[1].decode(), match.groups()[-1] == b'1'
            assert not sample['exhausted']
            assert sample['bindings'] == sample['matched'] + sample['replaced'] + sample['occupied']
            assert sample['matched'] == sample['closed'] + sample['current'] + sample['stale']
            assert sample['occupied'] <= sample['peak'] <= 16
            samples.append(sample)
            lines.append(line.decode('ascii'))
        assert len(samples) >= 2 and samples[0]['phase'] == 'bootstrap'
        assert samples[-1]['bindings'] > 0
        coverage = all(not s['capacity'] and not s['unmatched'] and not s['failed'] for s in samples)
        cases.append({'index': row['index'], 'traffic_preflight_pass': row['pass'],
            'payload': row['payload'], 'captures': captures, 'origin_samples': samples,
            'origin_coverage_pass': coverage, 'public_origin_markers': lines,
            'public_traffic_markers': row['public_markers'],
            'rx_stop': row['rx_stop'], 'rx_rebuild': row['rx_rebuild'],
            'initial_session_rejected': b'RFDBG_NET0_INITIAL_SESSION_REJECTED\r\n' in target})
    for source, name in ((args.build / 'rx-origin.json', 'rx-origin.json'),
                         (args.build / 'storage.json', 'storage.json'),
                         (args.build / 'Cargo.lock', 'sta.Cargo.lock'),
                         (args.build / 'rust-toolchain.toml', 'rust-toolchain.toml')):
        (HERE / name).write_bytes(source.read_bytes())
    flash = args.raw / 'target/riscv32imfc-unknown-none-elf/release'
    (HERE / 'flash-plan.json').write_bytes((flash / 'sta.plan.json').read_bytes())
    for name in ('rx-origin', 'storage', 'native-copy-paths'):
        report = json.loads((HERE / (name + '.json')).read_bytes())
        assert report['elf_sha256'] == ELF and report['status'] == 'pass'
    downloads = json.loads((HERE / 'download-verification.json').read_bytes())
    assert downloads['source'] == SOURCE and downloads['successful_jobs'] == 23
    capture_tool = HERE.parent / 'net0-rx-rebuild-2026-09-10/capture-tool.py'
    assert capture_tool.read_bytes() == (args.raw.parent / 'net0-rebuild-hil.py').read_bytes()
    config = HERE.parents[3] / 'examples/ws63/hil_wifi_config.rs'
    result = {'schema': 'net0-rx-origin-hil/v1', 'source_commit': SOURCE,
        'recorded_capture_source_label': raw['source_commit'], 'build_inputs': inputs,
        'elf_sha256': ELF, 'ap_elf_sha256': AP, 'raw_summary_sha256': sha(original),
        'image_sha256': sha((flash / 'sta.img').read_bytes()), 'flash': raw['flash'],
        'capture_tool': {'path': '../net0-rx-rebuild-2026-09-10/capture-tool.py',
                         'sha256': sha(capture_tool.read_bytes())},
        'public_paired_config': {'path': 'examples/ws63/hil_wifi_config.rs', 'sha256': sha(config.read_bytes())},
        'build_features': ['wpa2-personal', 'smoltcp', 'standard-l2-rx-origin-experiment',
            'standard-l2-rx-stop-experiment', 'incremental-connect-profile', 'bootstrap-stage-diag', 'firmware-example'],
        'reset_policy': raw['reset_policy'], 'runs': cases,
        'traffic_preflight_passes': sum(row['traffic_preflight_pass'] for row in cases),
        'origin_coverage_passes': sum(row['origin_coverage_pass'] for row in cases),
        'boundary': 'Observation-only prototype. Failed coverage and initial-session rejection are retained; no generation propagation, DMA fence or reconnect acceptance'}
    (HERE / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
    (HERE / 'SHA256SUMS').write_text(''.join(f'{sha(path.read_bytes())}  {path.name}\n'
        for path in sorted(HERE.iterdir()) if path.is_file() and path.name != 'SHA256SUMS'))
    print(json.dumps({key: result[key] for key in ('traffic_preflight_passes', 'origin_coverage_passes')}))


if __name__ == '__main__':
    main()
