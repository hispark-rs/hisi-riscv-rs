#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32", "gmssl==3.2.2", "pycryptodomex==3.23.0"]
# ///
"""Run SYMC HAL model cases against fixed sources; never admit hardware."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import ws53_ci_qemu_uart_clock_build as policy
import ws53_symc_model_build as builder

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / 'scripts/test-ws53-symc-model.py'

def run(args):
    source = args.model_source
    reference = source / 'scripts/ws53_symc_reference.py'
    committed = subprocess.run(['git', '-C', str(source), 'show', 'HEAD:scripts/ws53_symc_reference.py'],
        check=True, capture_output=True).stdout
    policy.require(reference.read_bytes() == committed, 'SYMC reference differs from committed source')
    admission = policy.validate_receipt(args.model_receipt, args.qemu, source_root=source)
    built = builder.validate(args.receipt, args.elf)
    before = {str(p): policy.digest(p) for p in (reference, RUNNER, Path(__file__), args.elf, args.receipt)}
    args.output.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location('symc_cpu_model_runner', RUNNER)
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.run(SimpleNamespace(qemu=args.qemu, elf=args.elf, reference=reference, output=args.output / 'cases'))
    cases = json.loads((args.output / 'cases/report.json').read_text())
    policy.require(cases['passed'] and cases['cases'] == 154, 'SYMC cases incomplete')
    policy.require(builder.validate(args.receipt, args.elf) == built, 'SYMC build changed during test')
    policy.require(policy.validate_receipt(args.model_receipt, args.qemu, source_root=source) == admission,
        'model changed during test')
    policy.require(all(policy.digest(Path(p)) == h for p, h in before.items()), 'test inputs changed')
    result = dict(passed=True, cases=cases['cases'], model=admission, inputs_sha256=before,
        case_report_sha256=policy.digest(args.output / 'cases/report.json'), hardware_accessed=False,
        physical_quiescence_proven=False, installed_firmware_qualified=False)
    (args.output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(dict(passed=True, cases=cases['cases'])))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    for name, value in vars(args).items():
        setattr(args, name, value.resolve())
    run(args)
