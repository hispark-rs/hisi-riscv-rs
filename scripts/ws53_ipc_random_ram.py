#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Controlled IPC18 image HIL record contract. Not a live peer/entropy proof."""
import hashlib
import struct
from elftools.elf.elffile import ELFFile

SIZE = 5440
CASES = [(0,128,0),(1,128,0),(2,128,0),(0,1,0),(0,3,0),(0,127,0),(2,6,0),
         (3,128,12),(0,128,14),(2,127,15),(0,128,11),(0,128,20),(0,128,31)]


def inspect(path, *, required=True):
    with path.open('rb') as stream:
        symbols = ELFFile(stream).get_section_by_name('.symtab').get_symbol_by_name('WS53_IPC_RANDOM_RAM')
        if not symbols and not required:
            return None
        if not symbols or len(symbols) != 1 or symbols[0]['st_size'] != SIZE:
            raise ValueError('missing/invalid IPC18 HIL report')
        address = symbols[0]['st_value']
        if address % 4 or not 0x20010000 <= address <= 0x20060000-SIZE:
            raise ValueError('IPC18 HIL report outside application RAM')
        return dict(address=address, size=SIZE)


def evaluate(data):
    if len(data) != SIZE or struct.unpack_from('<8I', data) != (0x524e4432,1,13,1,0,0,0,0):
        raise ValueError('IPC18 incomplete/invalid report')
    rows = []
    for i, (kind,length,status) in enumerate(CASES):
        record = data[32+i*416:32+(i+1)*416]
        words = struct.unpack_from('<8I', record)
        before, after, scratch = record[32:160], record[160:288], record[288:416]
        if words[:3] != (kind,length,status) or before != b'\xa5'*128:
            raise ValueError(f'IPC18 case {i} inputs/result')
        parsed = i < 7 or i >= 11
        offset = length//2 if kind == 2 and parsed else 0
        selected = (length if kind == 0 else length//2) if parsed else 0
        if words[3:5] != (offset,selected) or words[6:] != (1 if i == 12 else 10000,127 if i == 11 else 128):
            raise ValueError(f'IPC18 case {i} selection/budget')
        written = words[5]
        if status == 0:
            if written or after[:offset] != before[:offset] or after[offset+selected:] != before[offset+selected:] \
                    or after[offset:offset+selected] != scratch[:selected] or scratch[selected:] != b'\x33'*(128-selected):
                raise ValueError(f'IPC18 case {i} copy/guards')
            # Accepted words cannot be zero/all-ones. Do not infer entropy or
            # require different calls to produce different accepted words.
            for p in range(0, selected-selected%4, 4):
                if scratch[p:p+4] in (bytes(4), b'\xff'*4):
                    raise ValueError(f'IPC18 case {i} rejected accepted word')
        else:
            if after != before or written not in ((0,4) if i == 12 else (0,)) \
                    or scratch[written:] != b'\x33'*(128-written):
                raise ValueError(f'IPC18 case {i} failed transaction changed target/tail')
        rows.append(dict(index=i,kind=kind,length=length,status=status,offset=offset,selected=selected,
            staged_prefix=written,destination_sha256=hashlib.sha256(after).hexdigest(),
            scratch_sha256=hashlib.sha256(scratch).hexdigest()))
    return dict(passed=True, cases=rows, raw_sha256=hashlib.sha256(data).hexdigest(),
                live_peer_qualified=False,entropy_quality_claimed=False)
