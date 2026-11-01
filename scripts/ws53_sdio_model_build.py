#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Source-bound WS53 SDIO model fixtures; never a hardware admission."""
import argparse
from pathlib import Path
import ws53_ipc_build as base

def configure(kind):
    if kind not in ('init', 'transfer', 'async'):
        raise ValueError('unsupported SDIO fixture')
    features = ['model-handoff'] + (['model-async'] if kind == 'async' else [])
    base.common.MANIFEST = 'examples/ws53-sdio-init/Cargo.toml'
    base.common.FEATURES = features
    base.BIN = f'sdio-{kind}-model'
    base.SCHEMA = f'ws53-sdio-{kind}-model-elf-v1'
    base.HELPERS = base.HELPERS + ('scripts/ws53_sdio_model_build.py',)
    def snapshot():
        data = base.common.source_snapshot(base.ROOT, base.common.clean_environment())
        data.pop('shared_ram_fixture_sources')
        data['explicit'] = {n: base.common.digest(base.ROOT / n) for n in (
            'rust-toolchain.toml', '.cargo/config.toml', 'examples/ws53-sdio-init/Cargo.lock')}
        return data
    def command(out):
        return ['cargo', '+' + base.common.toolchain(base.ROOT), 'build', '--release', '--locked', '--offline',
            '-Zbuild-std=core,alloc', '--message-format=json-render-diagnostics',
            '--manifest-path', base.common.MANIFEST, '--features', ','.join(features),
            '--bin', base.BIN, '--target', base.common.TARGET, '--target-dir', str(out / 'cargo-target')]
    base.snapshot = snapshot
    base.command = command
    return base

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kind', choices=('init', 'transfer', 'async'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    configure(args.kind).build(args.output.resolve())
