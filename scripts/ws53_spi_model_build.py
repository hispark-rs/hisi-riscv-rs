#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Source-bound SPI wire fixture build, independent of hardware tooling."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import ws53_spi_wire as contract
from ws53_qemu_fixture import elf_check

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = 'examples/ws53/ram_smoke/Cargo.toml'
FEATURES = ['spi-wire-probe']
SCHEMA = 'ws53-spi-wire-model-build-v1'
HELPERS = ('scripts/ws53_spi_model_build.py', 'scripts/ws53_spi_wire.py',
           'scripts/ws53_ci_elf_build.py', 'scripts/check-ws53-ram-elf.py')
spec = importlib.util.spec_from_file_location('spi_model_build_common', ROOT/'scripts/ws53_ci_elf_build.py')
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
common.MANIFEST = MANIFEST
common.FEATURES = FEATURES


def snapshot():
    data = common.source_snapshot(ROOT, common.clean_environment())
    data.pop('shared_ram_fixture_sources')
    data['explicit'] = {name: common.digest(ROOT/name) for name in
                        ('rust-toolchain.toml', '.cargo/config.toml', 'examples/ws53/Cargo.lock')}
    return data


def command(out):
    return ['cargo', '+'+common.toolchain(ROOT), 'build', '--offline', '--locked', '--release',
            '-Zbuild-std=core,alloc', '--manifest-path', MANIFEST, '--features', ','.join(FEATURES),
            '--target', common.TARGET, '--target-dir', str(out/'cargo-target')]


def validate(receipt, elf):
    elf, out = elf.resolve(), receipt.resolve().parent
    record = json.loads(receipt.read_text())
    common.require(record['passed'] and record['schema'] == SCHEMA and record['sources'] == snapshot(),
                   'SPI fixture source receipt differs')
    common.require(record['helpers'] == {name: common.digest(ROOT/name) for name in HELPERS}
                   and record['command'] == command(out) and elf == out/'firmware.elf'
                   and record['elf_sha256'] == common.digest(elf)
                   and record['build_log_sha256'] == common.digest(out/'build.log')
                   and record['layout'] == elf_check.validate(elf)
                   and record['contract'] == contract.inspect(elf), 'SPI fixture build identity differs')
    return record


def build(out):
    out.mkdir(parents=True, exist_ok=False)
    before = snapshot()
    helpers = {name: common.digest(ROOT/name) for name in HELPERS}
    with (out/'build.log').open('wb') as log:
        result = subprocess.run(command(out), cwd=ROOT, env=common.clean_environment(), stdout=log, stderr=subprocess.STDOUT)
    common.require(result.returncode == 0, 'SPI fixture build failed; preserve log')
    common.require(before == snapshot() and helpers == {name: common.digest(ROOT/name) for name in HELPERS},
                   'SPI fixture inputs changed during build')
    elf = out/'firmware.elf'
    shutil.copyfile(out/'cargo-target'/common.TARGET/'release/ws53_ram_smoke', elf)
    record = dict(passed=True, schema=SCHEMA, sources=before, helpers=helpers, command=command(out),
                  elf_sha256=common.digest(elf), build_log_sha256=common.digest(out/'build.log'),
                  layout=elf_check.validate(elf), contract=contract.inspect(elf), hardware_accessed=False)
    (out/'receipt.json').write_text(json.dumps(record, indent=2)+'\n')
    validate(out/'receipt.json', elf)
    print(json.dumps(dict(passed=True, elf_sha256=record['elf_sha256'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    build(parser.parse_args().output.resolve())
