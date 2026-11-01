#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Run fixed SFC read HAL scenarios; no flash hardware admission."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
from types import SimpleNamespace
import ws53_ci_qemu_uart_clock_build as policy
import ws53_sfc_model_build as builder

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / 'scripts/ws53_sfc_hal_scenarios.py'
MODEL_HELPERS = ('scripts/ws53_sfc_contract_test.py', 'scripts/ws53_sfc_snapshot_test.py',
    'scripts/hisi_priority_test.py', 'scripts/ws53_branch_immediate_test.py')

def run(args):
    helpers = [args.model_source / name for name in MODEL_HELPERS]
    for name, path in zip(MODEL_HELPERS, helpers):
        committed = subprocess.run(['git', '-C', str(args.model_source), 'show', f'HEAD:{name}'],
            check=True, capture_output=True).stdout
        policy.require(path.read_bytes() == committed, 'SFC model helper differs from committed source')
    admission = policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source)
    built = builder.validate(args.receipt, args.elf)
    inputs = {str(p): policy.digest(p) for p in (*helpers, RUNNER, Path(__file__), args.elf, args.receipt)}
    args.output.mkdir(parents=True, exist_ok=False)
    spec = importlib.util.spec_from_file_location('sfc_cpu_model_runner', RUNNER)
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    cases = runner.run(SimpleNamespace(qemu=args.qemu, elf=args.elf, model_source=args.model_source,
        output=args.output / 'cases'))
    policy.require(cases['passed'] and len(cases['cases']) == 8, 'SFC scenarios incomplete')
    policy.require(builder.validate(args.receipt, args.elf) == built, 'fixture changed during test')
    policy.require(policy.validate_receipt(args.model_receipt, args.qemu, source_root=args.model_source) == admission,
        'model changed during test')
    policy.require(all(policy.digest(Path(p)) == h for p, h in inputs.items()), 'test inputs changed')
    result = dict(passed=True, cases=len(cases['cases']), model=admission, inputs_sha256=inputs,
        case_report_sha256=policy.digest(args.output / 'cases/report.json'), hardware_accessed=False,
        physical_quiescence_proven=False, flash_boot_qualified=False)
    (args.output / 'report.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(dict(passed=True, cases=len(cases['cases']))))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('qemu', 'model-receipt', 'model-source', 'elf', 'receipt', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    for name, value in vars(args).items():
        setattr(args, name, value.resolve())
    run(args)
