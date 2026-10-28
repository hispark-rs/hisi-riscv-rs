#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Reproducible QEMU-only I2C virtual-peer fixture with real pad routes."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
from ws53_qemu_fixture import elf_check

ROOT = Path(__file__).resolve().parents[1]
FEATURES = ['wire-pads']
MANIFEST = 'examples/ws53/i2c_control/Cargo.toml'
spec = importlib.util.spec_from_file_location('i2c_traffic_common', ROOT/'scripts/ws53_ci_elf_build.py')
common = importlib.util.module_from_spec(spec); spec.loader.exec_module(common)
common.MANIFEST = MANIFEST
common.FEATURES = FEATURES
HELPERS = ('scripts/ws53_i2c_traffic_build.py',
    'scripts/ws53_ci_elf_build.py','scripts/check-ws53-ram-elf.py')
SCHEMA = 'ws53-i2c-wire-traffic-build-v1'


def select_fault(scenario):
    feature = {'arbitration': 'arbitration-smoke', 'stuck': 'stuck-smoke', 'mixed': 'mixed-abort-smoke'}[scenario]
    if feature not in FEATURES:
        FEATURES.append(feature)


def select_arbitration():
    select_fault('arbitration')


def snapshot():
    data = common.source_snapshot(ROOT, common.clean_environment())
    data.pop('shared_ram_fixture_sources')
    data['explicit'] = {n:common.digest(ROOT/n) for n in (
        'rust-toolchain.toml','.cargo/config.toml','examples/ws53/Cargo.lock')}
    return data


def command(out):
    return ['cargo','+'+common.toolchain(ROOT),'build','--offline','--locked','--release',
        '-Zbuild-std=core,alloc','--manifest-path',MANIFEST,'--features',','.join(FEATURES),
        '--target',common.TARGET,'--target-dir',str(out/'cargo-target')]


def validate(receipt,elf):
    elf = elf.resolve()
    out = receipt.resolve().parent
    r = json.loads(receipt.read_text())
    common.require(r['passed'] and r['schema'] == SCHEMA and r['sources'] == snapshot(),
        'I2C traffic source receipt differs')
    common.require(r['helpers'] == {name:common.digest(ROOT/name) for name in HELPERS}
        and r['command'] == command(out) and elf.resolve() == out/'firmware.elf'
        and r['elf_sha256'] == common.digest(elf)
        and r['build_log_sha256'] == common.digest(out/'build.log')
        and r['layout'] == elf_check.validate(elf),'I2C traffic build identity differs')
    return r


def build(out):
    out.mkdir(parents=True,exist_ok=False)
    before = snapshot()
    helpers = {name:common.digest(ROOT/name) for name in HELPERS}
    with (out/'build.log').open('wb') as log:
        result = subprocess.run(command(out),cwd=ROOT,env=common.clean_environment(),
            stdout=log,stderr=subprocess.STDOUT)
    common.require(result.returncode == 0,'I2C traffic build failed; preserve log')
    common.require(before == snapshot() and helpers == {
        name:common.digest(ROOT/name) for name in HELPERS},'I2C traffic inputs changed')
    elf = out/'firmware.elf'
    shutil.copyfile(out/'cargo-target'/common.TARGET/'release/ws53_i2c_control',elf)
    r = dict(passed=True,schema=SCHEMA,sources=before,helpers=helpers,command=command(out),
        elf_sha256=common.digest(elf),build_log_sha256=common.digest(out/'build.log'),
        layout=elf_check.validate(elf),hardware_accessed=False,physical_fixture_admitted=False)
    (out/'receipt.json').write_text(json.dumps(r,indent=2)+'\n')
    validate(out/'receipt.json',elf)
    print(json.dumps(dict(passed=True,elf_sha256=r['elf_sha256'])))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--arbitration',action='store_true')
    p.add_argument('--fault',choices=('stuck','mixed'))
    args = p.parse_args()
    if args.arbitration and args.fault:
        p.error('choose one fault')
    if args.arbitration:
        select_arbitration()
    elif args.fault:
        select_fault(args.fault)
    build(args.output.resolve())
