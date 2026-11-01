#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Decode actual WS53 SPI0 driver traffic from the source-bound digital model."""
import argparse
import hashlib
import json
from pathlib import Path
import ws53_spi_wire as contract
import ws53_spi_wire_trace as trace
import ws53_spi_model_build as builder
import ws53_ci_qemu_uart_clock_build as policy
from ws53_qemu_fixture import run_fixture


def digest(path):
    with Path(path).open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def run(args):
    builder.validate(args.receipt, args.elf)
    info = contract.inspect(args.elf)
    admission = policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source)
    args.output.mkdir(parents=True, exist_ok=False)
    pad = args.output.resolve()/'pads.csv'
    expected = b'WS53 RAM data/bss OK\r\nWS53 PASS timer_irq=53 count=3\r\n'+contract.END
    gpio = [f'qemu-system-riscv32: ws53-gpio: opaque SW_CTL observation enabled (width={width}); selector encoding and silicon reset are not modeled'
            for width in (23, 12, 3)]
    result = run_fixture(str(args.qemu), args.elf, device='uart-pads', begin='', passed='',
        done=contract.END.decode().rstrip('\n'), expected_output=expected,
        expected_diagnostics=['ws53-spi: assumed profile 32MHz FIFO8 XOR-peer enable-crossing=1us',
            'ws53-spi: digital SCK/MOSI and routed MISO; analog/sample delay/native CS unmodeled']+gpio,
        qemu_options=('-global', 'ws53-spi.assume-profile=on', '-global', 'ws53-spi.assume-wire-profile=on',
            '-global', 'ws53-pinctrl.assume-spi-wire=on', '-global', 'ws53-gpio.assume-source-control=on',
            '-chardev', f'file,id=spitrace,path={pad}', '-global', 'ws53-pinctrl.pad-trace=spitrace'),
        ram_regions=((info['address'], 352),))
    observation = contract.evaluate(result['ram_snapshots'][0]['words'])
    waves = trace.analyze(pad, observation)
    builder.validate(args.receipt, args.elf)
    if policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source) != admission:
        raise ValueError('SPI model admission changed during execution')
    inputs = (Path(__file__), Path(contract.__file__), Path(trace.__file__), Path(builder.__file__),
              Path(policy.__file__), Path(__file__).with_name('ws53_qemu_fixture.py'),
              args.elf, args.receipt, args.model_receipt, pad)
    report = dict(passed=True, model_admission=admission, result=result, observation=observation, waves=waves,
        hardware_accessed=False, analog_timing_verified=False, external_slave_verified=False,
        qspi_verified=False, flash_qualified=False, cold_boot_verified=False,
        inputs_sha256={str(path.resolve()): digest(path) for path in inputs})
    (args.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(passed=True, frames=waves['frames'], bits=waves['bits'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    run(parser.parse_args())
