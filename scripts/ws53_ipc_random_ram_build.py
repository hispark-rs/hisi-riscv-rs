#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Source-bound RAM-only IPC18 image fixture for controlled HIL/QEMU."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import ws53_ipc_random_ram as contract

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('ipc_random_ram_engine', ROOT/'scripts/ws53_ci_elf_build.py')
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
common.MANIFEST = 'examples/ws53/ram_smoke/Cargo.toml'
FEATURES = ['ipc-random-smoke']
common.FEATURES = FEATURES
SCHEMA = 'ws53-ipc-random-ram-build-v1'
EXTRA_CONTRACT = None
HELPERS = ('scripts/ws53_ipc_random_ram_build.py','scripts/ws53_ipc_random_ram.py',
    'scripts/ws53_ci_elf_build.py','scripts/check-ws53-ram-elf.py',
    'scripts/test-ws53-uart-host.py','hil/ws53-ram-smoke.py')


def snapshot():
    data = common.source_snapshot(ROOT, common.clean_environment())
    data.pop('shared_ram_fixture_sources')
    data['explicit'] = {n:common.digest(ROOT/n) for n in (
        'rust-toolchain.toml','.cargo/config.toml','examples/ws53/Cargo.lock')}
    return data


def command(out):
    return ['cargo','+'+common.toolchain(ROOT),'build','--offline','--locked','--release','-Zbuild-std=core,alloc',
        '--manifest-path',common.MANIFEST,'--features',','.join(FEATURES),'--target',common.TARGET,
        '--target-dir',str(out/'cargo-target')]


def validate(receipt, elf):
    out = receipt.resolve().parent
    r = json.loads(receipt.read_text())
    common.require(r['passed'] and r['schema'] == SCHEMA and r['sources'] == snapshot(), 'RAM source receipt')
    common.require(r['helpers'] == {n:common.digest(ROOT/n) for n in HELPERS}, 'RAM helpers changed')
    common.require(r['command'] == command(out) and r['elf_sha256'] == common.digest(elf)
        and elf.resolve() == out/'firmware.elf' and r['build_log_sha256'] == common.digest(out/'build.log'), 'RAM build identity')
    common.require(r['contract'] == contract.inspect(elf), 'RAM report identity')
    common.require(r.get('extra_contract') == (EXTRA_CONTRACT(elf) if EXTRA_CONTRACT else None), 'RAM extra report identity')
    return r


def build(out):
    out.mkdir(parents=True,exist_ok=False)
    before = snapshot()
    helpers = {n:common.digest(ROOT/n) for n in HELPERS}
    with (out/'build.log').open('wb') as log:
        result = subprocess.run(command(out),cwd=ROOT,env=common.clean_environment(),stdout=log,stderr=subprocess.STDOUT)
    common.require(result.returncode == 0, 'RAM build failed, preserve log')
    common.require(before == snapshot() and helpers == {n:common.digest(ROOT/n) for n in HELPERS}, 'RAM inputs changed')
    elf = out/'firmware.elf'
    shutil.copyfile(out/'cargo-target'/common.TARGET/'release/ws53_ram_smoke', elf)
    from ws53_qemu_fixture import elf_check
    record = dict(passed=True,schema=SCHEMA,sources=before,helpers=helpers,
        command=command(out),elf_sha256=common.digest(elf),build_log_sha256=common.digest(out/'build.log'),
        contract=contract.inspect(elf),extra_contract=EXTRA_CONTRACT(elf) if EXTRA_CONTRACT else None,
        layout=elf_check.validate(elf),hardware_accessed=False,
        live_peer_qualified=False,flash_qualification=False)
    (out/'receipt.json').write_text(json.dumps(record,indent=2)+'\n')
    validate(out/'receipt.json',elf)
    print(json.dumps(dict(passed=True,elf_sha256=record['elf_sha256'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    build(parser.parse_args().output.resolve())
