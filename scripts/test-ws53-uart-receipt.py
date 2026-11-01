#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Reject stale/tampered UART receipts and wrong-source DWARF attribution."""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import elf_panic_sites
import ws53_uart_rx_build as builder


def rejects(operation, fragment):
    try:
        operation()
    except ValueError as error:
        assert fragment in str(error), str(error)
    else:
        raise AssertionError('invalid input accepted: '+fragment)


def run(receipt_path):
    elf = receipt_path.parent/'firmware.elf'
    original = json.loads(receipt_path.read_text())
    builder.validate(receipt_path, elf)
    # Keep the real receipt/ELF unchanged; validation binds its parent path.
    with tempfile.NamedTemporaryFile(dir=receipt_path.parent, suffix='.json', mode='w+') as temporary:
        path = Path(temporary.name)
        for key, value, message in (
            ('schema', 'ws53-flash-uart-rx-build-v1', 'field changed: schema'),
            ('inputs_unchanged', False, 'field changed: inputs_unchanged'),
            ('elf_sha256', '0'*64, 'ELF changed'),
            ('helper_sha256', {}, 'helper changed'),
            ('fault_binding', {}, 'fault attribution changed'),
        ):
            data = copy.deepcopy(original)
            data[key] = value
            path.write_text(json.dumps(data))
            rejects(lambda: builder.validate(path, elf), message)
    sites = {}
    hashes = {}
    for index, (name, (source, message)) in enumerate(builder.SITES.items(), 1):
        lines = (builder.ROOT/source).read_text().splitlines()
        line = next(i+1 for i, value in enumerate(lines) if value.strip() == f'panic!("{message}");')
        sites[name] = (source, line, index, message)
        hashes[source] = elf_panic_sites.digest(builder.ROOT/source)
    elf_panic_sites.validate(elf, builder.ROOT, sites, hashes)
    # Identical basenames, content and line numbers elsewhere must NOT bind to
    # this ELF's compilation directories. A basename-only parser accepts this.
    with tempfile.TemporaryDirectory(prefix='uart-wrong-dwarf-root-') as temporary:
        wrong_root = Path(temporary)
        for source in hashes:
            dest = wrong_root/source
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(builder.ROOT/source, dest)
        rejects(lambda: elf_panic_sites.validate(elf, wrong_root, sites, hashes), 'absent from ELF')
    bad_hashes = {name:'0'*64 for name in hashes}
    rejects(lambda: elf_panic_sites.validate(elf, builder.ROOT, sites, bad_hashes), 'source changed')
    spec = importlib.util.spec_from_file_location('uart_qemu_runner', Path(__file__).with_name('test-ws53-flash-uart-rx-qemu.py'))
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    rejects(lambda: runner.base.object_addresses(elf, {'WS53_FLASH_UART_RX_PHASE':8}, ram=(0x20050000,0x2005ff00)), 'invalid diagnostic')
    rejects(lambda: runner.base.object_addresses(elf, {'WS53_FLASH_UART_RX_PHASE':4}, ram=(0,4)), 'invalid diagnostic')
    print(json.dumps({'passed':True, 'negative_cases':9, 'valid_receipt':True, 'valid_dwarf':True, 'hardware_accessed':False}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--receipt', type=Path, required=True)
    args = parser.parse_args()
    run(args.receipt.resolve())
