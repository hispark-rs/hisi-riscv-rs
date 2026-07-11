#!/usr/bin/env python3
"""Generate the WS63 mask-ROM instruction patch table in a final ELF.

The vendor firmware performs the same operation after link: every linked
``foo_patch`` whose ``foo`` entry lies in mask ROM becomes one hardware compare
entry and one two-instruction long-jump stub. Deriving pairs from the final ELF
keeps the table valid when rust-lld changes the application layout.
"""

from __future__ import annotations

import argparse
import json
import struct
import subprocess
from pathlib import Path

ROM_START = 0x0010_9000
ROM_END = 0x0014_C000
PATCH_VMA = 0x0014_C000
PATCH_SIZE = 0x928
REMAP_ENTRY_COUNT = 194
INSTRUCTION_COMPARE_COUNT = 192
# The first two long-jump slots belong to FLPLACMP0/1 data/load patches. The
# instruction comparator's entry 0 maps to remap slot 2 in the vendor format.
DATA_PATCH_ENTRY_COUNT = 2
COMPARE_OFFSET = REMAP_ENTRY_COUNT * 8
COMPARE_HEADER_WORDS = 3


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--llvm-nm", type=Path, required=True)
    parser.add_argument("--rom-symbols", type=Path, required=True)
    parser.add_argument("--patch-list", type=Path, required=True)
    parser.add_argument("--expected-count", type=int)
    parser.add_argument("--report", type=Path, required=True)
    return parser.parse_args()


def read_symbols(llvm_nm: Path, elf: Path) -> dict[str, int]:
    output = subprocess.run(
        [str(llvm_nm), "--defined-only", "-n", str(elf)],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout
    symbols: dict[str, int] = {}
    for line in output.splitlines():
        fields = line.split()
        if len(fields) < 3:
            continue
        try:
            address = int(fields[0], 16)
        except ValueError:
            continue
        symbols[fields[-1]] = address
    return symbols


def read_rom_symbols(path: Path) -> dict[str, int]:
    symbols: dict[str, int] = {}
    for line in path.read_text().splitlines():
        statement = line.split("/*", 1)[0].strip().removesuffix(";")
        if "=" not in statement:
            continue
        name, value = (part.strip() for part in statement.split("=", 1))
        if not name or not value.startswith("0x"):
            continue
        try:
            address = int(value, 16)
        except ValueError:
            continue
        if ROM_START <= (address & ~1) < ROM_END:
            symbols[name] = address
    return symbols


def read_patch_names(path: Path) -> list[str]:
    names = [
        line.strip()
        for line in path.read_text().splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if len(names) != len(set(names)):
        raise ValueError(f"duplicate entry in {path}")
    return names


def elf_section(data: bytes, wanted: str) -> tuple[int, int, int, int]:
    if data[:4] != b"\x7fELF" or data[4] != 1 or data[5] != 1:
        raise ValueError("expected a little-endian ELF32 image")
    header = struct.unpack_from("<HHIIIIIHHHHHH", data, 16)
    section_offset = header[5]
    section_entry_size = header[10]
    section_count = header[11]
    string_table_index = header[12]
    if section_entry_size != 40 or string_table_index >= section_count:
        raise ValueError("unsupported ELF32 section table")

    sections = [
        struct.unpack_from("<IIIIIIIIII", data, section_offset + index * section_entry_size)
        for index in range(section_count)
    ]
    strings = sections[string_table_index]
    string_data = data[strings[4] : strings[4] + strings[5]]

    for section in sections:
        name_end = string_data.find(b"\0", section[0])
        name = string_data[section[0] : name_end].decode("ascii")
        if name == wanted:
            section_type, address, offset, size = section[1], section[3], section[4], section[5]
            return section_type, address, offset, size
    raise ValueError(f"ELF has no {wanted} section")


def encode_long_jump(source: int, destination: int) -> tuple[int, int]:
    offset = (destination & ~1) - (source & ~1)
    low = offset & 0xFFF
    high = offset & 0xFFFFF000
    if low > 0x7FF:
        high = (high + 0x1000) & 0xFFFFF000
    auipc_x6 = 0x17 | (6 << 7) | high
    jalr_x0_x6 = 0x67 | (6 << 15) | (low << 20)
    return auipc_x6 & 0xFFFF_FFFF, jalr_x0_x6 & 0xFFFF_FFFF


def discover_pairs(
    symbols: dict[str, int], rom_symbols: dict[str, int], patch_names: list[str]
) -> list[tuple[str, int, int]]:
    pairs = []
    for original_name in patch_names:
        patch_name = f"{original_name}_patch"
        patch_address = symbols.get(patch_name)
        if patch_address is None:
            raise ValueError(f"final ELF is missing required replacement {patch_name}")
        original_address = rom_symbols.get(original_name)
        if original_address is None:
            raise ValueError(f"ROM symbol table is missing {original_name}")
        if ROM_START <= (patch_address & ~1) < ROM_END:
            raise ValueError(f"{patch_name} unexpectedly resolves inside mask ROM")
        pairs.append((original_name, original_address, patch_address))
    pairs.sort(key=lambda item: (item[1] & ~1, item[0]))
    return pairs


def build_table(pairs: list[tuple[str, int, int]]) -> bytearray:
    if not pairs:
        raise ValueError("final ELF contains no mask-ROM patch pairs")
    if len(pairs) > INSTRUCTION_COMPARE_COUNT:
        raise ValueError(f"{len(pairs)} patches exceed the 192-entry controller")

    table = bytearray(PATCH_SIZE)
    for index, (_, original, replacement) in enumerate(pairs):
        stub = encode_long_jump(original, replacement)
        struct.pack_into("<II", table, (DATA_PATCH_ENTRY_COUNT + index) * 8, *stub)

    struct.pack_into("<III", table, COMPARE_OFFSET, 0, PATCH_VMA, len(pairs))
    for index, (_, original, _) in enumerate(pairs):
        struct.pack_into(
            "<I",
            table,
            COMPARE_OFFSET + (COMPARE_HEADER_WORDS + index) * 4,
            (original & ~1) | 1,
        )
    return table


def main() -> None:
    args = parse_args()
    elf_data = bytearray(args.elf.read_bytes())
    section_type, address, offset, size = elf_section(elf_data, ".patch")
    if section_type != 1 or address != PATCH_VMA or size != PATCH_SIZE:
        raise ValueError(
            f"unexpected .patch layout: type={section_type} address={address:#x} size={size:#x}"
        )

    symbols = read_symbols(args.llvm_nm, args.elf)
    rom_symbols = read_rom_symbols(args.rom_symbols)
    patch_names = read_patch_names(args.patch_list)
    pairs = discover_pairs(symbols, rom_symbols, patch_names)
    if args.expected_count is not None and len(pairs) != args.expected_count:
        raise ValueError(f"found {len(pairs)} ROM patches, expected {args.expected_count}")

    table = build_table(pairs)
    elf_data[offset : offset + size] = table
    args.elf.write_bytes(elf_data)
    if args.elf.read_bytes()[offset : offset + size] != table:
        raise OSError("post-write .patch verification failed")

    report = {
        "schema_version": 1,
        "elf": str(args.elf),
        "patch_vma": PATCH_VMA,
        "patch_size": PATCH_SIZE,
        "entry_count": len(pairs),
        "entries": [
            {
                "original": name,
                "original_address": original & ~1,
                "replacement": f"{name}_patch",
                "replacement_address": replacement & ~1,
            }
            for name, original, replacement in pairs
        ],
    }
    args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(f"generated {len(pairs)} WS63 mask-ROM patches in {args.elf}")


if __name__ == "__main__":
    main()
