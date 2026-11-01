#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Bind panic source locations to executable ELF instructions, not basename matches."""
import hashlib
from pathlib import Path
from elftools.elf.elffile import ELFFile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(elf, root, sites, source_hashes):
    """Bind reviewed panic lines using full DWARF paths and executable bytes."""
    for name, expected in source_hashes.items():
        require(digest(root/name) == expected, 'CI fault source changed; review required: '+name)
    for name,(source,line,_,text) in sites.items():
        require((root/source).read_text().splitlines()[line-1].strip() == 'panic!("'+text+'");',
                'panic source attribution changed: '+name)
    observed = {name:set() for name in sites}
    with elf.open('rb') as stream:
        image = ELFFile(stream)
        require(image.has_dwarf_info(), 'CI fault attribution requires compiler debug line information')
        executable = [(s['p_vaddr'],s['p_vaddr']+s['p_filesz']) for s in image.iter_segments()
                      if s['p_type'] == 'PT_LOAD' and s['p_flags'] & 1]
        dwarf = image.get_dwarf_info()
        for cu in dwarf.iter_CUs():
            program = dwarf.line_program_for_CU(cu)
            if program is None:
                continue
            attr = cu.get_top_DIE().attributes.get('DW_AT_comp_dir')
            require(attr is not None,'DWARF lacks compilation directory')
            comp_dir = Path(attr.value.decode())
            require(comp_dir.is_absolute(),'DWARF compilation directory is ambiguous')
            def filename(index):
                version=program.header.version
                slot=index-(0 if version >= 5 else 1)
                entries=program.header.file_entry
                require(0 <= slot < len(entries),'invalid DWARF file index')
                entry=entries[slot]
                name=Path(entry.name.decode())
                if name.is_absolute():
                    return name.resolve()
                directory=int(entry.dir_index)
                if version < 5 and directory == 0:
                    base=comp_dir
                else:
                    directories=program.header.include_directory
                    slot=directory-(0 if version >= 5 else 1)
                    require(0 <= slot < len(directories),'invalid DWARF directory index')
                    base=Path(directories[slot].decode())
                    if not base.is_absolute():
                        base=comp_dir/base
                return (base/name).resolve()
            for entry in program.get_entries():
                row = entry.state
                if row is None or row.end_sequence:
                    continue
                for name,(source,line,_,_) in sites.items():
                    if row.line == line and filename(row.file) == (root/source).resolve():
                        require(any(start <= row.address < end for start,end in executable),
                                'panic line is not executable ELF code')
                        observed[name].add(row.address)
    require(all(observed.values()),'compiled reviewed panic sites absent from ELF')
    return dict(source_sha256=dict(source_hashes),elf_sha256=digest(elf),
        dwarf_lines_verified=True,compiled_elf_binding_verified=True,
        executed_on_hardware=False,
        sites={name:dict(source=source,line=line,failure_code=code,message=text,
                        addresses=sorted(observed[name]))
               for name,(source,line,code,text) in sites.items()})
