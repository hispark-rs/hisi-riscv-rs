#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Decode bounded terminal pool reads for the one fixed observation ELF."""
import argparse
import hashlib
import json
from pathlib import Path
import struct

from elftools.elf.elffile import ELFFile

ELF_SHA = '21aabd008af78cf3094f1498ab15f788ff7cc37b25dc8bae46935e3a46ed3ccf'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--raw', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if sha(args.elf.read_bytes()) != ELF_SHA:
        raise ValueError('This receipt belongs only to its fixed HIL ELF')
    records = []

    def read(name, address, size):
        data = (args.raw / name).read_bytes()
        if len(data) != size:
            raise ValueError('wrong read length: ' + name)
        records.append(dict(name=name, address=address, bytes=size, sha256=sha(data)))
        return data

    with args.elf.open('rb') as stream:
        elf = ELFFile(stream)
        table = elf.get_section_by_name('.symtab')

        def address(name):
            values = table.get_symbol_by_name(name) or []
            if len(values) != 1:
                raise ValueError('missing/ambiguous symbol: ' + name)
            return values[0]['st_value']

        # These are the actual sequential probe read addresses, not inferred
        # adjacency of unrelated Rust/C objects.
        if (address('g_netbuf_pool') != 0x18244c
                or address('g_netbuf_max_size') != 0xa32be4
                or address('g_netbuf_size') != 0xa32be8):
            raise ValueError('captured addresses differ from ELF symbols')
        maximum, used = struct.unpack('<II', read('terminal-ram-budget.bin', 0xa32be4, 8))
        pointer, = struct.unpack('<I', read('terminal-pool-pointer.bin', 0x18244c, 4))
        if pointer != 0xa34780:
            raise ValueError('pool pointer differs from captured header address')
        header = read('terminal-pool-header.bin', pointer, 28)
        max_block, count = struct.unpack_from('<HH', header)
        packet_start, packet_end, pools, subpool_address, control = struct.unpack_from('<IIIII', header, 8)
        if (pools != 4 or count != 35 or subpool_address != 0xa3479c
                or control != 0xa34820 or packet_start >= packet_end):
            raise ValueError('unexpected pool layout; do not guess another ABI')
        raw = read('terminal-subpools.bin', subpool_address, pools * 20)
        subpools, next_index = [], 0
        for index in range(pools):
            free, total, minimum, reserved, start, size, packet, head = struct.unpack_from('<HHHHHHII', raw, index * 20)
            if (not 0 <= minimum <= free <= total or reserved > total
                    or start != next_index or size == 0 or size > max_block
                    or packet < packet_start or packet + total * size > packet_end
                    or head and not (control <= head <= control + count * 16 and (head - control) % 16 == 0)
                    or free and not control + start * 16 <= head < control + (start + total) * 16):
                raise ValueError('invalid bounded subpool record')
            next_index += total
            subpools.append(dict(index=index, free=free, total=total, min_free=minimum,
                reserved=reserved, start_index=start, block_bytes=size,
                packet_address=packet, free_head=head, historical_peak_used=total - minimum))
        if next_index != count:
            raise ValueError('subpool counts do not conserve pool capacity')
        result = dict(schema='net0-buffer-owner-postmortem/v1', elf_sha256=ELF_SHA,
            captures=records, pool=dict(address=pointer, max_block_bytes=max_block,
                controls=count, control_stride=16, control_address=control,
                packet_start=packet_start, packet_end=packet_end, subpools=subpools),
            ram=dict(configured_max_bytes=maximum, observed_live_bytes=used,
                default_budget_from_sdk_assembly=0x6c00),
            boundary='Sequential terminal reads, not an atomic fence. Per-subpool historical peaks need not coincide. Zero RAM usage now does not prove no earlier RAM fallback; its byte budget is not an accepted buffer-count bound.')
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'controls': count, 'free': sum(p['free'] for p in subpools),
                      'ram_live_bytes': used, 'captures': len(records)}))


if __name__ == '__main__':
    main()
