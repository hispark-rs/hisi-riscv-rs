#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Run actual WS53 I2C driver against a selected synthetic abort status."""
import argparse
import hashlib
import json
from pathlib import Path
import ws53_i2c_traffic_build as builder
import ws53_ci_qemu_i2c_wire_build as policy
from ws53_qemu_fixture import run_fixture


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(args):
    builder.select_fault(args.scenario)
    builder.validate(args.receipt, args.elf)
    admission = policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source)
    args.output.mkdir(parents=True, exist_ok=False)
    marker = {'arbitration': 'arbitration_loss', 'stuck': 'stuck', 'mixed': 'mixed_abort'}[args.scenario]
    ready = b'WS53 I2C clear fault\n'
    signals = {'arbitration': ('arbitration',), 'stuck': ('inject-sda-stuck',),
               'mixed': ('inject-sda-stuck', 'nack-address')}[args.scenario]
    passed = f'WS53 PASS I2C {marker}=1 raw=1 retry=1 other_instance=1'
    result = run_fixture(str(args.qemu), args.elf, device='i2c', fault='i2c-'+args.scenario,
        begin='WS53 I2C QEMU begin', done='WS53 I2C QEMU done',
        passed=passed, input_stages=((ready, b'R'),), i2c_stages=tuple((ready, signal, 0) for signal in signals),
        output_validator=lambda output: output.decode().splitlines() == [
            'WS53 I2C QEMU begin', 'WS53 I2C clear fault', passed, 'WS53 I2C QEMU done'],
        qemu_options=('-global', 'ws53-i2c.assume-profile=on',
            '-global', 'ws53-i2c.assume-wire-profile=on', '-global', 'ws53-pinctrl.assume-i2c-wire=on'),
        expected_diagnostics=['ws53-i2c: assumed32MHz master wire/virtual-peer profile; pull/filter delay unmodeled',
            'ws53-i2c: assumed profile 32MHz FIFO8 CON-addressing deferred-read-ACK disable=1us'])
    builder.validate(args.receipt, args.elf)
    if policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source) != admission:
        raise ValueError('model admission changed during arbitration test')
    inputs = (Path(__file__), Path(builder.__file__), Path(policy.__file__),
              Path(__file__).with_name('ws53_qemu_fixture.py'), args.elf, args.receipt, args.model_receipt)
    report = dict(passed=True, scenario=args.scenario, hardware_accessed=False, physical_qualification=False,
                  model_admission=admission, result=result, inputs_sha256={str(p.resolve()):digest(p) for p in inputs})
    (args.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(passed=True, scenario=args.scenario, fault_attempts=2, unaffected_bus=1)))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        p.add_argument('--'+name, type=Path, required=True)
    p.add_argument('--scenario', choices=('arbitration', 'stuck', 'mixed'), default='arbitration')
    run(p.parse_args())
