#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Reproducible QEMU-only I2C virtual-peer fixture with real pad routes."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import ws53_ipc_random_ram_build as engine
from ws53_qemu_fixture import elf_check

ROOT = Path(__file__).resolve().parents[1]
FEATURES = ['wire-pads']
MANIFEST = 'examples/ws53/i2c_control/Cargo.toml'
engine.common.MANIFEST = MANIFEST
engine.common.FEATURES = FEATURES
HELPERS = ('scripts/ws53_i2c_traffic_build.py','scripts/ws53_ipc_random_ram_build.py',
    'scripts/ws53_ci_elf_build.py','scripts/check-ws53-ram-elf.py')
SCHEMA = 'ws53-i2c-wire-traffic-build-v1'


def command(out):
    return ['cargo','+'+engine.common.toolchain(ROOT),'build','--offline','--locked','--release',
        '-Zbuild-std=core,alloc','--manifest-path',MANIFEST,'--features',','.join(FEATURES),
        '--target',engine.common.TARGET,'--target-dir',str(out/'cargo-target')]


def validate(receipt,elf):
    elf = elf.resolve()
    out = receipt.resolve().parent
    r = json.loads(receipt.read_text())
    engine.common.require(r['passed'] and r['schema'] == SCHEMA and r['sources'] == engine.snapshot(),
        'I2C traffic source receipt differs')
    engine.common.require(r['helpers'] == {name:engine.common.digest(ROOT/name) for name in HELPERS}
        and r['command'] == command(out) and elf.resolve() == out/'firmware.elf'
        and r['elf_sha256'] == engine.common.digest(elf)
        and r['build_log_sha256'] == engine.common.digest(out/'build.log')
        and r['layout'] == elf_check.validate(elf),'I2C traffic build identity differs')
    return r


def build(out):
    out.mkdir(parents=True,exist_ok=False)
    before = engine.snapshot()
    helpers = {name:engine.common.digest(ROOT/name) for name in HELPERS}
    with (out/'build.log').open('wb') as log:
        result = subprocess.run(command(out),cwd=ROOT,env=engine.common.clean_environment(),
            stdout=log,stderr=subprocess.STDOUT)
    engine.common.require(result.returncode == 0,'I2C traffic build failed; preserve log')
    engine.common.require(before == engine.snapshot() and helpers == {
        name:engine.common.digest(ROOT/name) for name in HELPERS},'I2C traffic inputs changed')
    elf = out/'firmware.elf'
    shutil.copyfile(out/'cargo-target'/engine.common.TARGET/'release/ws53_i2c_control',elf)
    r = dict(passed=True,schema=SCHEMA,sources=before,helpers=helpers,command=command(out),
        elf_sha256=engine.common.digest(elf),build_log_sha256=engine.common.digest(out/'build.log'),
        layout=elf_check.validate(elf),hardware_accessed=False,physical_fixture_admitted=False)
    (out/'receipt.json').write_text(json.dumps(r,indent=2)+'\n')
    validate(out/'receipt.json',elf)
    print(json.dumps(dict(passed=True,elf_sha256=r['elf_sha256'])))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,required=True)
    build(p.parse_args().output.resolve())
