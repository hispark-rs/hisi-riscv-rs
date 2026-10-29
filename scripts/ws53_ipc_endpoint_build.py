#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Source-bound build of the polling-endpoint model fixture; never hardware admission."""
import argparse
from pathlib import Path
import ws53_ipc_spsc_build as spsc

base = spsc.base
base.SCHEMA = 'ws53-ipc-endpoint-model-elf-v1'
base.HELPERS = base.HELPERS + ('scripts/ws53_ipc_endpoint_build.py',)
base.common.FEATURES = ['model-handoff', 'endpoint-contract']
original_command = base.command

def command(out):
    args = original_command(out)
    args[args.index('--features') + 1] = 'model-handoff,endpoint-contract'
    return args

base.command = command
validate = base.validate
build = base.build

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    build(parser.parse_args().output.resolve())
