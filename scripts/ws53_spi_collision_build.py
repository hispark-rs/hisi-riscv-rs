#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Source-bound actual-driver fixture for synthetic SPI status-only faults."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
from ws53_qemu_fixture import elf_check

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = 'examples/ws53/spi_control/Cargo.toml'
HELPERS = ('scripts/ws53_spi_collision_build.py', 'scripts/ws53_ci_elf_build.py', 'scripts/check-ws53-ram-elf.py')
SCHEMA = 'ws53-spi-collision-build-v1'
spec = importlib.util.spec_from_file_location('spi_collision_common', ROOT/'scripts/ws53_ci_elf_build.py')
common = importlib.util.module_from_spec(spec)
spec.loader.exec_module(common)
common.MANIFEST = MANIFEST


def configure(bus):
    common.FEATURES = [f'collision-{bus}']


def snapshot():
    data = common.source_snapshot(ROOT, common.clean_environment())
    data.pop('shared_ram_fixture_sources')
    data['explicit'] = {name: common.digest(ROOT/name) for name in
                        ('rust-toolchain.toml', '.cargo/config.toml', 'examples/ws53/Cargo.lock')}
    return data


def command(out):
    return ['cargo', '+'+common.toolchain(ROOT), 'build', '--offline', '--locked', '--release',
            '-Zbuild-std=core,alloc', '--manifest-path', MANIFEST, '--features', ','.join(common.FEATURES),
            '--target', common.TARGET, '--target-dir', str(out/'cargo-target')]


def validate(receipt, elf):
    elf, out = elf.resolve(), receipt.resolve().parent
    record = json.loads(receipt.read_text())
    common.require(record['passed'] and record['schema'] == SCHEMA and record['sources'] == snapshot(),
                   'SPI collision fixture source differs')
    common.require(record['helpers'] == {name: common.digest(ROOT/name) for name in HELPERS}
                   and record['command'] == command(out) and elf == out/'firmware.elf'
                   and record['elf_sha256'] == common.digest(elf)
                   and record['build_log_sha256'] == common.digest(out/'build.log')
                   and record['layout'] == elf_check.validate(elf), 'SPI collision build identity differs')
    return record


def build(out):
    out.mkdir(parents=True, exist_ok=False)
    before, helpers = snapshot(), {name: common.digest(ROOT/name) for name in HELPERS}
    with (out/'build.log').open('wb') as log:
        result = subprocess.run(command(out), cwd=ROOT, env=common.clean_environment(), stdout=log, stderr=subprocess.STDOUT)
    common.require(result.returncode == 0, 'SPI collision fixture build failed; preserve log')
    common.require(before == snapshot() and helpers == {name: common.digest(ROOT/name) for name in HELPERS},
                   'SPI collision build inputs changed')
    elf = out/'firmware.elf'
    shutil.copyfile(out/'cargo-target'/common.TARGET/'release/ws53_spi_control', elf)
    record = dict(passed=True, schema=SCHEMA, sources=before, helpers=helpers, command=command(out),
                  elf_sha256=common.digest(elf), build_log_sha256=common.digest(out/'build.log'),
                  layout=elf_check.validate(elf), hardware_accessed=False)
    (out/'receipt.json').write_text(json.dumps(record, indent=2)+'\n')
    validate(out/'receipt.json', elf)
    print(json.dumps(dict(passed=True, elf_sha256=record['elf_sha256'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bus', choices=('spi0', 'qspi1'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    configure(args.bus)
    build(args.output.resolve())
