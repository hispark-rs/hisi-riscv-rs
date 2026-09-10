#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Read exact compiler layout; never guess Rust field ordering from source."""
import argparse
import hashlib
import json
from pathlib import Path
from elftools.elf.elffile import ELFFile

p = argparse.ArgumentParser()
p.add_argument('elf')
p.add_argument('--snapshot', type=Path)
p.add_argument('--snapshot-address', type=lambda value: int(value, 0))
p.add_argument('--original-snapshot', type=Path)
p.add_argument('--original-address', type=lambda value: int(value, 0))
p.add_argument('--output', type=Path)
a = p.parse_args()
with open(a.elf, 'rb') as stream:
    elf = ELFFile(stream)
    dwarf = elf.get_dwarf_info()
    found = []
    for cu in dwarf.iter_CUs():
        for die in cu.iter_DIEs():
            attr = die.attributes.get('DW_AT_name')
            name = attr.value.decode(errors='replace') if attr and isinstance(attr.value, bytes) else ''
            if die.tag != 'DW_TAG_structure_type' or not (
                    'Tracker<16>' in name or name == 'RxOriginDiagnostics'):
                continue
            fields = []
            for child in die.iter_children():
                if child.tag != 'DW_TAG_member':
                    continue
                member = child.attributes
                member_type = child.get_DIE_from_attribute('DW_AT_type')
                type_name = member_type.attributes.get('DW_AT_name')
                fields.append(dict(name=member['DW_AT_name'].value.decode(),
                    offset=member['DW_AT_data_member_location'].value,
                    type=type_name.value.decode() if type_name else member_type.tag))
            found.append(dict(name=name, bytes=die.attributes['DW_AT_byte_size'].value, fields=fields))
    result = found
    if a.snapshot:
        def layout(name):
            candidates = [item for item in found if item['name'] == name]
            if not candidates or any(item != candidates[0] for item in candidates):
                raise ValueError('missing/inconsistent compiler layout: ' + name)
            return candidates[0]

        def offsets(item):
            return {field['name']: field['offset'] for field in item['fields']}

        def symbol(name):
            items = elf.get_section_by_name('.symtab').get_symbol_by_name(name) or []
            if len(items) != 1:
                raise ValueError('missing/ambiguous symbol: ' + name)
            return items[0]

        tracker = layout('Tracker<16>')
        cell = layout('RefCell<hisi_rf_ws63::netif_l2::rx_origin::Tracker<16>>')
        diagnostic = layout('RxOriginDiagnostics')
        storage = symbol('__hisi_net0_rx_origins')
        original = symbol('__hisi_net0_original_free')
        if (storage['st_size'] != cell['bytes'] or original['st_size'] != 4
                or a.snapshot_address != storage['st_value']
                or a.original_address != original['st_value']):
            raise ValueError('snapshot address/size differs from the fixed ELF')
        raw = a.snapshot.read_bytes()
        original_raw = a.original_snapshot.read_bytes()
        if len(raw) != cell['bytes'] or len(original_raw) != 4:
            raise ValueError('snapshot length mismatch')
        start = offsets(cell)['value']
        diag = start + offsets(tracker)['diagnostic']
        values = {}
        for field in diagnostic['fields']:
            size = {'u64': 8, 'usize': 4, 'bool': 1}[field['type']]
            offset = diag + field['offset']
            values[field['name']] = int.from_bytes(raw[offset:offset + size], 'little')
        pointer = int.from_bytes(original_raw, 'little')
        if pointer != symbol('oal_mem_netbuf_free_from_ram')['st_value']:
            raise ValueError('captured original differs from the fixed ELF native free')
        conserved = (values['bindings'] == values['matched'] + values['replaced']
                     + values['retired_on_free_attempt'] + values['occupied']
                     and values['matched'] == sum(values[k] for k in ('closed_origin', 'current_origin', 'stale_origin')))
        result = dict(schema='net0-origin-postmortem/v1',
            elf_sha256=hashlib.sha256(Path(a.elf).read_bytes()).hexdigest(),
            snapshot_sha256=hashlib.sha256(raw).hexdigest(), bytes=len(raw),
            address=storage['st_value'], original_free=pointer,
            original_snapshot_sha256=hashlib.sha256(original_raw).hexdigest(),
            original_address=original['st_value'], original_bytes=4,
            layout=[cell, tracker, diagnostic], diagnostics=values,
            observed_conservation=conserved,
            boundary='Sequential read-only postmortem after the failed capture; not an atomic snapshot or a HIL pass')
    text = json.dumps(result, indent=2) + '\n'
    if a.output:
        a.output.write_text(text)
    print(text, end='')
