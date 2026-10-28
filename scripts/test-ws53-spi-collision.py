#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Run actual WS53 SPI driver against synthetic status-only collision."""
import argparse
import hashlib
import json
from pathlib import Path
import ws53_spi_collision_build as builder
import ws53_ci_qemu_uart_clock_build as policy
from ws53_qemu_fixture import run_fixture


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(args):
    builder.configure(args.bus)
    builder.validate(args.receipt, args.elf)
    admission = policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source)
    args.output.mkdir(parents=True, exist_ok=False)
    channel = {'spi0': 0, 'qspi1': 1}[args.bus]
    ready = b'WS53 SPI clear collision\n'
    passed = f'WS53 PASS SPI status_collision={args.bus} irq_absent=1 recovered=1 peer=1'
    result = run_fixture(str(args.qemu), args.elf, device='spi', fault=f'spi-status{channel}',
        begin='WS53 SPI QEMU begin', done='WS53 SPI QEMU done', passed=passed,
        input_stages=((ready, b'R'),), spi_stages=((ready, channel),),
        output_validator=lambda output: output.decode().splitlines() == [
            'WS53 SPI QEMU begin', 'WS53 SPI clear collision', passed, 'WS53 SPI QEMU done'],
        qemu_options=('-global', 'ws53-spi.assume-profile=on'),
        expected_diagnostics=['ws53-spi: assumed profile 32MHz FIFO8 XOR-peer enable-crossing=1us'])
    builder.validate(args.receipt, args.elf)
    if policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source) != admission:
        raise ValueError('SPI collision model identity changed')
    inputs = (Path(__file__), Path(builder.__file__), Path(policy.__file__),
              Path(__file__).with_name('ws53_qemu_fixture.py'), args.elf, args.receipt, args.model_receipt)
    report = dict(passed=True, bus=args.bus, hardware_accessed=False, physical_qualification=False,
                  model_admission=admission, result=result, inputs_sha256={str(p.resolve()): digest(p) for p in inputs})
    (args.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(passed=True, bus=args.bus, status_only=True, recovered=True)))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--bus', choices=('spi0', 'qspi1'), required=True)
    run(parser.parse_args())
