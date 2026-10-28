#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Execute fixed HASH/KM model scenarios, without hardware admission."""
import argparse
from contextlib import redirect_stdout
import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace
import ws53_ci_qemu_uart_clock_build as policy
import ws53_crypto_model_build as build

ROOT = Path(__file__).resolve().parents[1]
COUNTS = {'hash': 198, 'km': 357}

def run(args):
    runner_path = ROOT / f'scripts/test-ws53-{args.kind}-model.py'
    builder = build.configure(args.kind)
    admission = policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source)
    built = builder.validate(args.receipt, args.elf)
    inputs = {str(p): policy.digest(p) for p in (runner_path, Path(__file__), args.elf, args.receipt)}
    args.output.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location(f'{args.kind}_cpu_model_runner', runner_path)
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    with (args.output / 'runner.log').open('w') as log, redirect_stdout(log):
        runner.run(SimpleNamespace(qemu=args.qemu, elf=args.elf, output=args.output / 'cases'))
    cases = json.loads((args.output / 'cases/report.json').read_text())
    count = len(cases['cases']) if args.kind == 'hash' else cases['cases']
    policy.require(cases['passed'] and count == COUNTS[args.kind], 'model scenarios incomplete')
    policy.require(builder.validate(args.receipt, args.elf) == built, 'fixture changed during test')
    policy.require(policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source) == admission,
        'model changed during test')
    policy.require(all(policy.digest(Path(p)) == h for p, h in inputs.items()), 'test inputs changed')
    result = dict(passed=True, kind=args.kind, cases=count, model=admission, inputs_sha256=inputs,
        case_report_sha256=policy.digest(args.output / 'cases/report.json'), hardware_accessed=False,
        physical_quiescence_proven=False, installed_firmware_qualified=False)
    (args.output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(dict(passed=True, kind=args.kind, cases=count)))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kind', choices=tuple(COUNTS), required=True)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    for name in ('qemu', 'model_receipt', 'model_source', 'elf', 'receipt', 'output'):
        setattr(args, name, getattr(args, name).resolve())
    run(args)
