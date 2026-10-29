#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Fail closed on WS53 MCU_MODE_1 RAM ELF layout/ISA violations; never loads it."""
import argparse
import hashlib
import io
import json
from pathlib import Path
from elftools.elf.elffile import ELFFile

PROGRAM = (0x28000, 0x30000)
DATA = (0x20050000, 0x2005FF00)


def require(condition, message):
    # These are hardware loading guards, not debug assertions: Python -O must
    # never disable them.
    if not condition:
        raise ValueError(message)


def validate(path):
    # Parse, bound and hash the same snapshot. A concurrent rebuild may replace
    # the path; opening again after parsing could otherwise hash another ELF.
    image = path.read_bytes()
    with io.BytesIO(image) as stream:
        elf = ELFFile(stream)
        require(elf.elfclass == 32 and elf.little_endian, "expected ELF32 little endian")
        require(elf["e_machine"] == "EM_RISCV", "expected RISC-V")
        require(elf["e_type"] == "ET_EXEC", "expected a linked executable")
        require(elf["e_flags"] == 3, "expected RVC + single-float ABI only")
        require(elf["e_entry"] == PROGRAM[0], "entry must be WS53 RAM reset_vector")
        loads = []
        file_loads = []
        for seg in elf.iter_segments():
            if seg["p_type"] != "PT_LOAD":
                continue
            start, size = seg["p_paddr"], seg["p_memsz"]
            require(start == seg["p_vaddr"], "RAM image cannot depend on relocation from flash")
            require(seg["p_filesz"] <= size, "file segment exceeds memory size")
            require(seg["p_offset"] + seg["p_filesz"] <= len(image), "truncated load segment")
            region = DATA if seg["p_flags"] & 2 else PROGRAM
            require(region[0] <= start <= start + size <= region[1], "segment outside owned RAM profile")
            require(not seg["p_flags"] & 2 or not seg["p_flags"] & 1, "writable executable segment")
            require(not any(start < other["address"] + other["size"] and
                            other["address"] < start + size for other in loads),
                    "overlapping load segments")
            loads.append({"address": start, "size": size, "flags": seg["p_flags"]})
            file_loads.append({"address": start, "size": seg["p_filesz"],
                               "offset": seg["p_offset"], "flags": seg["p_flags"]})
        require(loads, "no loadable segments")
        require(any(s["flags"] & 1 and s["address"] <= elf["e_entry"] < s["address"] + s["size"]
                    for s in file_loads), "entry is not file-backed executable")
        # Executable sections contain RVC 16-bit or base 32-bit instructions.
        # All AMO/LR/SC share opcode 0x2f; reject it rather than relying on target flags.
        for sec in elf.iter_sections():
            if not sec["sh_flags"] & 4:
                continue
            require(sec["sh_type"] == "SHT_PROGBITS", "executable section is not file-backed")
            require(any(s["flags"] & 1 and s["address"] <= sec["sh_addr"] and
                        sec["sh_addr"] + sec["sh_size"] <= s["address"] + s["size"] and
                        sec["sh_offset"] == s["offset"] + sec["sh_addr"] - s["address"]
                        for s in file_loads), "executable section does not match file-backed load segment")
            data, offset = sec.data(), 0
            while offset < len(data):
                half = int.from_bytes(data[offset:offset + 2], "little")
                width = 2 if half & 3 != 3 else 4
                require(offset + width <= len(data), "truncated instruction")
                require(not (width == 4 and half & 0x1f == 0x1f), "unsupported long instruction")
                require(not (width == 4 and half & 0x7f == 0x2f), f"atomic instruction in {sec.name}+{offset:#x}")
                offset += width
        symtab = elf.get_section_by_name(".symtab")
        require(symtab is not None, "retain symbols for RAM debugging")
        symbols = {s.name: s["st_value"] for s in symtab.iter_symbols()}
        require(symbols.get("reset_vector") == PROGRAM[0], "invalid reset_vector")
        trap = symbols.get("trap_vector", 0)
        require(PROGRAM[0] <= trap < PROGRAM[1] and trap % 256 == 0, "invalid trap_vector")
        for name in ("__stack_top__", "__irq_stack_top__", "__exc_stack_top__", "__nmi_stack_top__"):
            value = symbols.get(name, 0)
            require(DATA[0] < value <= DATA[1] and value % 16 == 0, f"invalid {name}")
    return {"profile": "ws53-acore-mcu-mode1-ram", "elf": str(path),
            "sha256": hashlib.sha256(image).hexdigest(), "loads": loads,
            "verified": ["ELF32", "ilp32f", "RAM-only", "no-A-instructions", "stack-boundaries"],
            "hardware_verified": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    args = parser.parse_args()
    print(json.dumps(validate(args.elf), indent=2))
