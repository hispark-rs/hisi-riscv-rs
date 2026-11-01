#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Source-bound WS53 HASH/KM model fixture; never a hardware admission."""
import argparse
from pathlib import Path
import ws53_ipc_build as base

def configure(kind):
    if kind not in ('hash', 'km'):
        raise ValueError('unsupported crypto fixture')
    example = f'ws53-{kind}-model'
    base.common.MANIFEST = f'examples/{example}/Cargo.toml'
    base.common.FEATURES = ['model-handoff']
    base.BIN = f'{kind}-model'
    base.SCHEMA = f'ws53-{kind}-model-elf-v1'
    base.HELPERS = base.HELPERS + ('scripts/ws53_crypto_model_build.py',)
    def snapshot():
        data = base.common.source_snapshot(base.ROOT, base.common.clean_environment())
        data.pop('shared_ram_fixture_sources')
        data['explicit'] = {n: base.common.digest(base.ROOT / n) for n in (
            'rust-toolchain.toml', '.cargo/config.toml', f'examples/{example}/Cargo.lock')}
        return data
    def command(out):
        return ['cargo', '+' + base.common.toolchain(base.ROOT), 'build', '--release', '--locked', '--offline',
            '-Zbuild-std=core,alloc', '--message-format=json-render-diagnostics',
            '--manifest-path', base.common.MANIFEST, '--features', 'model-handoff',
            '--bin', base.BIN, '--target', base.common.TARGET, '--target-dir', str(out / 'cargo-target')]
    base.snapshot = snapshot
    base.command = command
    return base

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kind', choices=('hash', 'km'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    configure(args.kind).build(args.output.resolve())
