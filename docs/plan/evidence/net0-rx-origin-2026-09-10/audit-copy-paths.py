#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Identify linked native-to-host and AMSDU allocation boundaries, without HIL."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

from elftools.elf.elffile import ELFFile

sys.dont_write_bytecode = True
EDGES = (
    ('frw_rx_netbuf', 'frw_alloc_pbuf', 1),
    ('frw_alloc_pbuf', 'oal_pbuf_netbuf_alloc', 1),
    ('hmac_rx_parse_amsdu_etc', 'oal_pbuf_netbuf_alloc', 1),
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--backend', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    checker = args.backend / '.github/scripts/check-net0-cleanup.py'
    spec = importlib.util.spec_from_file_location('native_calls', checker)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    result = module.inspect(args.elf, EDGES)
    result['rejected_call_mutations'] = module.tamper(args.elf, EDGES)
    with args.elf.open('rb') as stream:
        elf = ELFFile(stream)
        symbols = elf.get_section_by_name('.symtab')
        target = symbols.get_symbol_by_name('oal_pbuf_netbuf_alloc')[0]['st_value']
        allocators = []
        for symbol in symbols.iter_symbols():
            if (symbol['st_info']['type'] != 'STT_FUNC' or not symbol['st_size']
                    or not isinstance(symbol['st_shndx'], int)):
                continue
            section = elf.get_section(symbol['st_shndx'])
            offset = symbol['st_value'] - section['sh_addr']
            if offset < 0 or offset + symbol['st_size'] > section['sh_size']:
                raise ValueError('function outside its section')
            sites = [site for site, callee in module.calls(
                section.data()[offset:offset + symbol['st_size']], symbol['st_value']) if callee == target]
            if sites:
                allocators.append({'function': symbol.name, 'call_addresses': sites})
    result.update(schema='net0-native-copy-audit/v1',
        checker_sha256=hashlib.sha256(checker.read_bytes()).hexdigest(),
        host_pbuf_allocator_callers=sorted(allocators, key=lambda row: row['function']),
        boundary='Static normalized AUIPC/JALR calls only. AMSDU allocation must preserve source epoch; this does not prove path execution, complete clone/free coverage, or reconnect safety')
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'edges': len(result['edges']),
                      'rejected_call_mutations': result['rejected_call_mutations'],
                      'allocator_callers': allocators}))


if __name__ == '__main__':
    main()
