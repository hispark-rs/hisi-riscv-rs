#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Run the real I2C driver against a virtual peer rejecting general call."""
import argparse
import hashlib
import json
from pathlib import Path
import ws53_i2c_traffic_build as builder
import ws53_ci_qemu_uart_clock_build as policy
from ws53_qemu_fixture import run_fixture


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(args):
    builder.select_fault('general-call')
    builder.validate(args.receipt, args.elf)
    admission = policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source)
    args.output.mkdir(parents=True, exist_ok=False)
    passed = 'WS53 PASS I2C general_call_nack=1 raw=1 retry=1 other_instance=1'
    result = run_fixture(str(args.qemu), args.elf, device='i2c',
        begin='WS53 I2C QEMU begin', done='WS53 I2C QEMU done', passed=passed,
        output_validator=lambda output: output.decode().splitlines() == [
            'WS53 I2C QEMU begin', passed, 'WS53 I2C QEMU done'],
        qemu_options=('-global', 'ws53-i2c.assume-profile=on',
            '-global', 'ws53-i2c.assume-wire-profile=on', '-global', 'ws53-pinctrl.assume-i2c-wire=on'),
        expected_diagnostics=['ws53-i2c: assumed32MHz master wire/virtual-peer profile; pull/filter delay unmodeled',
            'ws53-i2c: assumed profile 32MHz FIFO8 CON-addressing deferred-read-ACK disable=1us'])
    builder.validate(args.receipt, args.elf)
    if policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source) != admission:
        raise ValueError('general call model identity changed')
    inputs = (Path(__file__), Path(builder.__file__), Path(policy.__file__),
              Path(__file__).with_name('ws53_qemu_fixture.py'), args.elf, args.receipt, args.model_receipt)
    report = dict(passed=True, scenario='general-call', hardware_accessed=False,
                  physical_qualification=False, model_admission=admission, result=result,
                  inputs_sha256={str(p.resolve()): digest(p) for p in inputs})
    (args.output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(passed=True, general_call_nack_attempts=2, unaffected_bus=1)))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    run(parser.parse_args())
