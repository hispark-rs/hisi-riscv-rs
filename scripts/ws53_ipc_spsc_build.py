#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Fixed concurrent IPC RAM fixture build; never hardware admission."""
import argparse
import importlib.util
from pathlib import Path
spec=importlib.util.spec_from_file_location('spsc_firmware_base',Path(__file__).with_name('ws53_ipc_build.py'))
base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)

base.BIN='ipc-spsc-model'
base.SCHEMA='ws53-ipc-spsc-model-elf-v1'
base.HELPERS=base.HELPERS+('scripts/ws53_ipc_spsc_build.py',)
validate=base.validate
build=base.build

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,required=True)
    build(p.parse_args().output.resolve())
