#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Source-bound UART clock model fixture; never hardware admission."""
import argparse
from pathlib import Path
import ws53_ipc_build as base

base.common.MANIFEST = 'examples/ws53/ram_smoke/Cargo.toml'
base.common.FEATURES = ['uart-clock-probe']
base.BIN = 'ws53_ram_smoke'
base.SCHEMA = 'ws53-uart-clock-model-elf-v2'
base.HELPERS += ('scripts/ws53_uart_clock_build.py',)

def snapshot():
    data = base.common.source_snapshot(base.ROOT, base.common.clean_environment())
    data.pop('shared_ram_fixture_sources')
    data['explicit'] = {n: base.common.digest(base.ROOT / n) for n in (
        'rust-toolchain.toml', '.cargo/config.toml', 'examples/ws53/Cargo.lock')}
    return data

def command(out):
    return ['cargo', '+' + base.common.toolchain(base.ROOT), 'build', '--release', '--locked', '--offline',
        '-Zbuild-std=core,alloc', '--message-format=json-render-diagnostics',
        '--manifest-path', base.common.MANIFEST, '--features', 'uart-clock-probe',
        '--bin', base.BIN, '--target', base.common.TARGET, '--target-dir', str(out / 'cargo-target')]

base.snapshot = snapshot
base.command = command
validate, build = base.validate, base.build

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    build(parser.parse_args().output.resolve())
