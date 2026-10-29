#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Source-bound IPC18/TRNG image fixture, never hardware admission."""
import argparse
import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location('random_firmware_base', Path(__file__).with_name('ws53_ipc_build.py'))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
base.BIN = 'ipc-random-model'
base.SCHEMA = 'ws53-ipc-random-model-elf-v1'
base.HELPERS += ('scripts/ws53_ipc_random_build.py',)
validate, build = base.validate, base.build

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    build(parser.parse_args().output.resolve())
