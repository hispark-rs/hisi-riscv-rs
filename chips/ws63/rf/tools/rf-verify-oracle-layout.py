#!/usr/bin/env python3
"""Verify that a patched RF link preserved the oracle layout.

`rf-patch-reloc58-from-oracle.py` bakes oracle-resolved addresses into the
vendor objects. That is only safe if the final rust-lld link places the patched
input sections at the same VMAs as the oracle link. This verifier compares the
patched relocation manifest against the final linker map and fails closed on any
layout drift.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import struct
import sys
from pathlib import Path


def parse_map(path: Path) -> dict[tuple[str, str, str], int]:
    entries: dict[tuple[str, str, str], int] = {}
    pending_section: str | None = None
    object_re = re.compile(r"(\S+\.a)\(([^)]+)\)")
    addr_re = re.compile(r"0x([0-9a-fA-F]+)\s+0x([0-9a-fA-F]+)")
    lld_re = re.compile(
        r"^\s*([0-9a-fA-F]+)\s+"
        r"([0-9a-fA-F]+)\s+"
        r"([0-9a-fA-F]+)\s+"
        r"\d+\s+"
        r"(\S+\.a)\(([^)]+)\):\(([^)]+)\)"
    )
    with path.open("r", encoding="utf-8", errors="replace") as f:
        for line in f:
            lld = lld_re.search(line)
            if lld:
                vma = int(lld.group(1), 16)
                size = int(lld.group(3), 16)
                if size == 0 or vma == 0:
                    continue
                archive = os.path.basename(lld.group(4))
                member = lld.group(5)
                section = lld.group(6)
                entries[(archive, member, section)] = vma
                continue

            stripped = line.strip()
            if not stripped:
                continue
            first = stripped.split()[0]
            if first.startswith(".") and "0x" not in stripped and ".a(" not in stripped:
                pending_section = first
                continue

            obj = object_re.search(stripped)
            addr = addr_re.search(stripped)
            if not obj or not addr:
                continue
            section = first if first.startswith(".") else pending_section
            if not section:
                continue
            archive = os.path.basename(obj.group(1))
            member = obj.group(2)
            vma = int(addr.group(1), 16)
            size = int(addr.group(2), 16)
            if size == 0 or vma == 0:
                continue
            entries[(archive, member, section)] = vma
    return entries


SELF_CALL = b"\x97\x00\x00\x00\xe7\x80\x00\x00"


def find_self_call_offsets(data: bytes) -> list[int]:
    """Find `auipc ra, 0; jalr ra, 0(ra)` placeholders."""
    offsets: list[int] = []
    start = 0
    while (offset := data.find(SELF_CALL, start)) >= 0:
        offsets.append(offset)
        start = offset + 2
    return offsets


def executable_self_calls(path: Path) -> list[tuple[str, int]]:
    """Return self-call VMAs from executable sections of an ELF32 LE image."""
    elf = path.read_bytes()
    if len(elf) < 52 or elf[:4] != b"\x7fELF" or elf[4:6] != b"\x01\x01":
        raise ValueError(f"{path} is not an ELF32 little-endian image")

    shoff = struct.unpack_from("<I", elf, 32)[0]
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", elf, 46)
    if shentsize < 40 or shstrndx >= shnum:
        raise ValueError(f"{path} has an invalid section table")

    sections: list[tuple[int, int, int, int, int]] = []
    for index in range(shnum):
        offset = shoff + index * shentsize
        if offset + 40 > len(elf):
            raise ValueError(f"{path} has a truncated section table")
        name, _kind, flags, addr, file_offset, size = struct.unpack_from(
            "<IIIIII", elf, offset
        )
        sections.append((name, flags, addr, file_offset, size))

    _, _, _, strings_offset, strings_size = sections[shstrndx]
    strings = elf[strings_offset : strings_offset + strings_size]

    def section_name(offset: int) -> str:
        end = strings.find(b"\0", offset)
        if end < 0:
            end = len(strings)
        return strings[offset:end].decode("utf-8", errors="replace")

    found: list[tuple[str, int]] = []
    for name_offset, flags, addr, file_offset, size in sections:
        if flags & 0x4 == 0 or size == 0:  # SHF_EXECINSTR
            continue
        body = elf[file_offset : file_offset + size]
        for offset in find_self_call_offsets(body):
            found.append((section_name(name_offset), addr + offset))
    return found


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--final-map", required=True, type=Path)
    parser.add_argument("--final-elf", required=True, type=Path)
    args = parser.parse_args()

    final_entries = parse_map(args.final_map)
    checked = 0
    errors: list[str] = []
    seen: set[tuple[str, str, str]] = set()

    with args.manifest.open("r", encoding="utf-8") as f:
        for line_no, line in enumerate(f, 1):
            item = json.loads(line)
            key = (item["archive"], item["member"], item["section"])
            if key in seen:
                continue
            seen.add(key)
            expected = int(item["section_vma"])
            actual = final_entries.get(key)
            if actual is None:
                errors.append(f"{line_no}: missing final map entry for {key!r}")
                continue
            if actual != expected:
                errors.append(
                    f"{line_no}: layout drift for {key!r}: "
                    f"oracle=0x{expected:08x} final=0x{actual:08x}"
                )
                continue
            checked += 1

    if errors:
        for err in errors[:80]:
            print(f"ERROR: {err}", file=sys.stderr)
        if len(errors) > 80:
            print(f"ERROR: ... {len(errors) - 80} more layout errors", file=sys.stderr)
        return 1

    if checked == 0:
        print("ERROR: no patched relocation sections were verified", file=sys.stderr)
        return 1

    try:
        self_calls = executable_self_calls(args.final_elf)
    except ValueError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    if self_calls:
        for section, address in self_calls:
            print(
                "ERROR: unresolved weak call encoded as self-call at "
                f"{section}+0x{address:08x}",
                file=sys.stderr,
            )
        return 1

    print(f"verified oracle/final RF layout sections: {checked}")
    print("verified executable sections contain no unresolved self-call placeholders")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
