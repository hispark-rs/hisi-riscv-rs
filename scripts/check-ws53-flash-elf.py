#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Validate the experimental WS53 FlashBoot ELF; optionally extract its payload.

This never constructs a combined/control-core image or touches hardware. Use
hisi-fwpkg against the qualified backup for packaging and hisiflash for writes.
Instruction scans do not prove complete control flow: unsymbolized gaps,
untrusted metadata and alternate instruction boundaries remain limitations.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import runpy
from elftools.elf.elffile import ELFFile

FLASH = (0x45d300, 0x670000)
RAM = (0x20050000, 0x2005ff00)
BOOTSTRAP = (0x2f000, 0x30000)


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def contains(region, start, size):
    return region[0] <= start < start + size <= region[1]


def validate(path, *, allow_ram_functions=False):
    # The relocation audit, main ELF checks, extracted payload and hash must
    # all describe the same input, even if a concurrent build replaces path.
    image = path.read_bytes()
    ram_functions = None
    if allow_ram_functions:
        audit = Path(__file__).resolve().parents[1]/'crates/hisi-riscv-rt/scripts/check-ws53-ram-functions.py'
        ram_functions = runpy.run_path(str(audit))['validate_bytes'](image)
    with io.BytesIO(image) as stream:
        elf = ELFFile(stream)
        require(elf.elfclass == 32 and elf.little_endian and elf['e_machine'] == 'EM_RISCV', 'ELF32 RISC-V required')
        require(elf['e_type'] == 'ET_EXEC' and elf['e_flags'] == 3, 'linked RVC/single-float ELF required')
        require(elf['e_entry'] == FLASH[0], 'wrong vendor application entry')
        loads, parts, file_offsets = [], [], []
        for segment in elf.iter_segments():
            if segment['p_type'] != 'PT_LOAD' or segment['p_memsz'] == 0:
                continue
            v, p = segment['p_vaddr'], segment['p_paddr']
            size, count, flags = segment['p_memsz'], segment['p_filesz'], segment['p_flags']
            require(count <= size and segment['p_offset']+count <= len(image), 'truncated load segment')
            require(not (flags & 1 and flags & 2), 'writable executable segment')
            if flags & 2:
                require(contains(RAM, v, size) or (count == 0 and contains(BOOTSTRAP, v, size)), 'RAM/stack ownership violation')
            else:
                ram_copy = ram_functions is not None and flags == 5 and v == ram_functions['begin'] \
                    and p == ram_functions['load'] and count == size == ram_functions['size']
                require(ram_copy or (v == p and contains(FLASH, v, size)), 'code/constant outside Flash')
            require(not any(v < other['vma']+other['memsz'] and other['vma'] < v+size for other in loads), 'overlapping virtual segments')
            if count:
                require(contains(FLASH, p, count), 'file bytes would require a debugger RAM load')
                require(not any(p < address+len(data) and address < p+count for address, data in parts), 'overlapping Flash payload')
                parts.append((p, segment.data()))
            loads.append(dict(vma=v, lma=p, filesz=count, memsz=size, flags=flags))
            file_offsets.append(segment['p_offset'])
        require(any(row['flags'] & 1 and row['vma'] == FLASH[0] and row['filesz'] > 0
                    for row in loads), 'entry not executable')
        scanned = []
        for section in elf.iter_sections():
            if not section['sh_flags'] & 4:
                continue
            require(section['sh_type'] == 'SHT_PROGBITS' and any(row['flags'] & 1 and
                row['vma'] <= section['sh_addr'] and section['sh_addr']+section['sh_size'] <= row['vma']+row['filesz']
                and section['sh_offset'] == offset + section['sh_addr'] - row['vma']
                for row, offset in zip(loads, file_offsets)), 'executable bytes outside file-backed Flash')
            scanned.append((section['sh_addr'], section['sh_addr']+section['sh_size']))
            if ram_functions is not None and section.name == '.ws53.ramtext':
                # This section intentionally includes placed read-only data.
                # The RAM audit scans function symbols and resolves references.
                continue
            data, offset = section.data(), 0
            while offset < len(data):
                half = int.from_bytes(data[offset:offset+2], 'little')
                width = 2 if half & 3 != 3 else 4
                require(offset+width <= len(data), 'truncated instruction')
                require(not (width == 4 and half & 0x1f == 0x1f), 'unsupported long instruction')
                require(not (width == 4 and half & 0x7f == 0x2f), 'unsupported atomic instruction')
                offset += width
        table = elf.get_section_by_name('.symtab')
        require(table is not None, 'debug symbols required')
        symbols = {s.name:s['st_value'] for s in table.iter_symbols()}
        def scanned_address(address):
            return any(start <= address < end for start, end in scanned)
        require(scanned_address(elf['e_entry']), 'entry missing from scanned executable sections')
        for symbol in table.iter_symbols():
            address = symbol['st_value']
            if symbol['st_info']['type'] == 'STT_FUNC' and any(
                    row['flags'] & 1 and row['vma'] <= address < row['vma']+row['filesz']
                    for row in loads):
                require(scanned_address(address), 'function missing from scanned executable sections')
        require(symbols.get('reset_vector') == FLASH[0], 'reset vector mismatch')
        require(symbols.get('__bootstrap_stack_bottom__') == BOOTSTRAP[0] and
                symbols.get('__bootstrap_stack_top__') == BOOTSTRAP[1], 'bootstrap stack outside retained ITCM')
        for name in ('__stack_top__', '__irq_stack_top__', '__exc_stack_top__', '__nmi_stack_top__'):
            value = symbols.get(name, 0)
            require(RAM[0] < value <= RAM[1] and value % 16 == 0, 'invalid '+name)
        for name in ('trap_vector', '__hisi_ws53_prepare_flash_ram', 'runtime_init'):
            value = symbols.get(name, 0)
            require(FLASH[0] <= value < FLASH[1] and (name != 'trap_vector' or value % 256 == 0), 'invalid '+name)
            require(scanned_address(value), name+' missing from scanned executable sections')
        data_section = elf.get_section_by_name('.data')
        require(data_section is not None and data_section['sh_size'] > 0, 'fixture must exercise initialized data')
        begin, end = symbols.get('__data_begin__', 0), symbols.get('__data_end__', 0)
        source = symbols.get('__data_load__', 0)
        require(begin == data_section['sh_addr'] and end-begin == data_section['sh_size'], 'data symbols disagree')
        require(contains(FLASH, source, end-begin) and contains(RAM, begin, end-begin), 'data LMA/VMA not separated')
        require(any(row['vma'] == begin and row['lma'] == source and row['filesz'] >= end-begin for row in loads), 'data not file-backed at expected LMA')
        payload_end = max(address+len(data) for address, data in parts)
        require(symbols.get('__flash_image_end__') == payload_end, 'Flash extent mismatch')
        payload = bytearray(b'\xff'*(payload_end-FLASH[0]))
        for address, data in parts:
            payload[address-FLASH[0]:address-FLASH[0]+len(data)] = data
    return dict(profile='ws53-vendor-flashboot-mode1-experimental', entry=FLASH[0],
        elf_sha256=hashlib.sha256(image).hexdigest(), loads=loads,
        payload_sha256=hashlib.sha256(payload).hexdigest(), payload_bytes=len(payload),
        verified=['ELF32/ilp32f', 'no-A-in-scanned-instructions', 'Flash-only-file-bytes', 'data-copy-layout', 'stack-boundaries'],
        hardware_verified=False, cold_boot_verified=False, complete_image=False,
        **({'ram_functions':{k:v for k,v in ram_functions.items() if k != 'symbols'}}
           if ram_functions is not None else {})), bytes(payload)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('elf', type=Path)
    p.add_argument('--binary', type=Path)
    p.add_argument('--ram-functions', action='store_true', help='require and audit opt-in RAM relocation')
    a = p.parse_args()
    report, binary = validate(a.elf, allow_ram_functions=a.ram_functions)
    if a.binary:
        with a.binary.open('xb') as dest:
            dest.write(binary)
    print(json.dumps(report, indent=2))
