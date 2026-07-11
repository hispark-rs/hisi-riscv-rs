#!/usr/bin/env python3
"""Patch HiSilicon R_RISCV_48_LLUI relocations out of WS63 RF archives.

The vendor Wi-Fi blobs use a HiSilicon-specific relocation type 58 for the
48-bit `l.li rd, imm32` instruction. Upstream LLVM/rust-lld does not know this
relocation, so final links fail before an ELF is produced.

This tool uses a vendor-linked oracle ELF plus its linker map to patch the same
immediate bytes into the original archive members, then turns relocation type
58 into R_RISCV_NONE. The resulting archive is still ordinary ELF/ar input for
rust-lld; the vendor linker is used only to produce the oracle.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import struct
import sys
from dataclasses import dataclass
from pathlib import Path

ELF_MAGIC = b"\x7fELF"
AR_MAGIC = b"!<arch>\n"
AR_HDR_SIZE = 60
R_RISCV_NONE = 0
R_RISCV_48_LLUI = 58
R_RISCV_LLUI_BRANCH = 59
R_RISCV_LLUI_REP = 61
SHT_RELA = 4
SHT_SYMTAB = 2
PT_LOAD = 1


@dataclass(frozen=True)
class Section:
    name: str
    sh_type: int
    offset: int
    size: int
    entsize: int
    link: int
    info: int


@dataclass(frozen=True)
class ProgramHeader:
    p_type: int
    offset: int
    vaddr: int
    paddr: int
    filesz: int
    memsz: int


def u16(buf: bytes | bytearray, off: int) -> int:
    return struct.unpack_from("<H", buf, off)[0]


def u32(buf: bytes | bytearray, off: int) -> int:
    return struct.unpack_from("<I", buf, off)[0]


def i32(buf: bytes | bytearray, off: int) -> int:
    return struct.unpack_from("<i", buf, off)[0]


def put_u32(buf: bytearray, off: int, value: int) -> None:
    struct.pack_into("<I", buf, off, value & 0xFFFF_FFFF)


def cstr(tab: bytes | bytearray, off: int) -> str:
    end = tab.find(b"\0", off)
    if end < 0:
        end = len(tab)
    return bytes(tab[off:end]).decode("utf-8", "replace")


def require_elf32_le(buf: bytes | bytearray, what: str) -> None:
    if not buf.startswith(ELF_MAGIC):
        raise ValueError(f"{what}: not an ELF file")
    if buf[4] != 1 or buf[5] != 1:
        raise ValueError(f"{what}: expected ELF32 little-endian")


def parse_sections(buf: bytes | bytearray, what: str) -> list[Section]:
    require_elf32_le(buf, what)
    shoff = u32(buf, 0x20)
    shentsize = u16(buf, 0x2E)
    shnum = u16(buf, 0x30)
    shstrndx = u16(buf, 0x32)
    if shentsize < 40:
        raise ValueError(f"{what}: unsupported section header size {shentsize}")
    raw = []
    for idx in range(shnum):
        off = shoff + idx * shentsize
        raw.append(
            (
                u32(buf, off + 0),
                u32(buf, off + 4),
                u32(buf, off + 16),
                u32(buf, off + 20),
                u32(buf, off + 36),
                u32(buf, off + 24),
                u32(buf, off + 28),
                u32(buf, off + 32),
            )
        )
    shstr = b""
    if shstrndx < len(raw):
        _, _, off, size, _, _, _, _ = raw[shstrndx]
        shstr = buf[off : off + size]
    sections = []
    for name_off, typ, off, size, entsize, link, info, _addralign in raw:
        sections.append(Section(cstr(shstr, name_off), typ, off, size, entsize, link, info))
    return sections


def parse_program_headers(buf: bytes | bytearray, what: str) -> list[ProgramHeader]:
    require_elf32_le(buf, what)
    phoff = u32(buf, 0x1C)
    phentsize = u16(buf, 0x2A)
    phnum = u16(buf, 0x2C)
    if phentsize < 32:
        raise ValueError(f"{what}: unsupported program header size {phentsize}")
    phdrs = []
    for idx in range(phnum):
        off = phoff + idx * phentsize
        phdrs.append(
            ProgramHeader(
                p_type=u32(buf, off + 0),
                offset=u32(buf, off + 4),
                vaddr=u32(buf, off + 8),
                paddr=u32(buf, off + 12),
                filesz=u32(buf, off + 16),
                memsz=u32(buf, off + 20),
            )
        )
    return phdrs


def elf_bytes_at_vma(elf: bytes | bytearray, phdrs: list[ProgramHeader], vma: int, size: int) -> bytes:
    for ph in phdrs:
        if ph.p_type != PT_LOAD:
            continue
        if ph.vaddr <= vma and vma + size <= ph.vaddr + ph.filesz:
            off = ph.offset + (vma - ph.vaddr)
            return bytes(elf[off : off + size])
    raise KeyError(f"oracle ELF has no file-backed LOAD bytes at VMA 0x{vma:08x}")


def parse_map(path: Path) -> dict[tuple[str, str, str], int]:
    """Return (archive basename, member, section name) -> output VMA."""
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


def ar_name(raw: bytes, long_names: bytes) -> str:
    name = raw.decode("utf-8", "replace").rstrip()
    if name.startswith("#1/"):
        raise ValueError("BSD extended ar names are not supported")
    if name.startswith("/") and name not in {"/", "//"}:
        off = int(name[1:])
        end = long_names.find(b"/\n", off)
        if end < 0:
            end = long_names.find(b"\n", off)
        return long_names[off:end].decode("utf-8", "replace")
    if name.endswith("/"):
        name = name[:-1]
    return name


def iter_ar_members(buf: bytes | bytearray):
    if not buf.startswith(AR_MAGIC):
        raise ValueError("not a GNU ar archive")
    pos = len(AR_MAGIC)
    long_names = b""
    while pos + AR_HDR_SIZE <= len(buf):
        hdr = bytes(buf[pos : pos + AR_HDR_SIZE])
        if hdr[58:60] != b"`\n":
            raise ValueError(f"bad ar header magic at offset {pos}")
        size = int(hdr[48:58].decode("ascii").strip() or "0")
        data_start = pos + AR_HDR_SIZE
        data_end = data_start + size
        name_field = hdr[:16]
        if name_field.decode("utf-8", "replace").rstrip() == "//":
            long_names = bytes(buf[data_start:data_end])
        name = ar_name(name_field, long_names)
        extra_name_len = 0
        if name_field.decode("utf-8", "replace").rstrip().startswith("#1/"):
            extra_name_len = int(name_field.decode("ascii").rstrip()[3:])
        yield name, data_start + extra_name_len, data_end
        pos = data_end + (size & 1)


def patch_object(
    obj: bytearray,
    archive_basename: str,
    member: str,
    map_entries: dict[tuple[str, str, str], int],
    oracle: bytes,
    oracle_phdrs: list[ProgramHeader],
    manifest: list[dict[str, int | str]],
) -> tuple[int, int, int]:
    sections = parse_sections(obj, f"{archive_basename}({member})")
    patched = 0
    missing = 0
    debug_relocs = 0
    lli_rep_relocs = 0
    for relsec in sections:
        if relsec.sh_type != SHT_RELA:
            continue
        if relsec.info >= len(sections):
            continue
        target = sections[relsec.info]
        if not target.name:
            continue
        if relsec.entsize == 0:
            entsize = 12
        else:
            entsize = relsec.entsize
        count = relsec.size // entsize
        if target.name.startswith(".debug"):
            for idx in range(count):
                ent = relsec.offset + idx * entsize
                r_info = u32(obj, ent + 4)
                if (r_info & 0xFF) != R_RISCV_NONE:
                    put_u32(obj, ent + 4, (r_info & ~0xFF) | R_RISCV_NONE)
                    debug_relocs += 1
            continue

        base_vma = map_entries.get((archive_basename, member, target.name))
        for idx in range(count):
            ent = relsec.offset + idx * entsize
            r_offset = u32(obj, ent + 0)
            r_info = u32(obj, ent + 4)
            r_type = r_info & 0xFF
            if r_type in (R_RISCV_LLUI_BRANCH, R_RISCV_LLUI_REP):
                # HiSilicon binutils uses these markers on already-encoded
                # custom instructions. Upstream lld assigns different meanings
                # to these numeric values and corrupts the instruction bytes.
                put_u32(obj, ent + 4, (r_info & ~0xFF) | R_RISCV_NONE)
                lli_rep_relocs += 1
                continue
            if r_type != R_RISCV_48_LLUI:
                continue
            _r_addend = i32(obj, ent + 8) if entsize >= 12 else 0
            if base_vma is None:
                missing += 1
                continue
            # `l.li` is 6 bytes: first halfword contains opcode+rd; the next
            # four bytes are the relocated imm32.
            oracle_imm = elf_bytes_at_vma(oracle, oracle_phdrs, base_vma + r_offset + 2, 4)
            obj[target.offset + r_offset + 2 : target.offset + r_offset + 6] = oracle_imm
            put_u32(obj, ent + 4, (r_info & ~0xFF) | R_RISCV_NONE)
            manifest.append(
                {
                    "archive": archive_basename,
                    "member": member,
                    "section": target.name,
                    "section_vma": base_vma,
                    "rel_offset": r_offset,
                    "rel_vma": base_vma + r_offset,
                    "imm32": int.from_bytes(oracle_imm, "little"),
                }
            )
            patched += 1
    return patched, missing, debug_relocs + lli_rep_relocs


def patch_archive(
    archive: Path,
    output: Path,
    map_entries: dict[tuple[str, str, str], int],
    oracle: bytes,
    oracle_phdrs: list[ProgramHeader],
    manifest: list[dict[str, int | str]],
) -> tuple[int, int, int]:
    shutil.copyfile(archive, output)
    buf = bytearray(output.read_bytes())
    archive_basename = archive.name
    total_patched = 0
    total_missing = 0
    total_neutralized_relocs = 0
    for member, data_start, data_end in iter_ar_members(buf):
        data = buf[data_start:data_end]
        if not data.startswith(ELF_MAGIC):
            continue
        obj = bytearray(data)
        patched, missing, neutralized_relocs = patch_object(
            obj, archive_basename, member, map_entries, oracle, oracle_phdrs, manifest
        )
        if patched or neutralized_relocs:
            buf[data_start:data_end] = obj
            total_patched += patched
            total_neutralized_relocs += neutralized_relocs
        total_missing += missing
    output.write_bytes(buf)
    return total_patched, total_missing, total_neutralized_relocs


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--map", required=True, type=Path)
    parser.add_argument("--oracle-elf", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument(
        "--manifest",
        type=Path,
        help="Write a JSONL manifest of patched relocation sites for final-layout verification.",
    )
    parser.add_argument(
        "--allow-missing-map",
        action="store_true",
        help=(
            "Allow relocation-58 entries in archive members that were not pulled "
            "into the oracle link. This is useful when patching normal archives "
            "for one rooted closure such as uapi_wifi_init."
        ),
    )
    parser.add_argument("archives", nargs="+", type=Path)
    args = parser.parse_args()

    map_entries = parse_map(args.map)
    oracle = args.oracle_elf.read_bytes()
    oracle_phdrs = parse_program_headers(oracle, str(args.oracle_elf))
    args.out_dir.mkdir(parents=True, exist_ok=True)

    total_patched = 0
    total_missing = 0
    total_neutralized_relocs = 0
    manifest: list[dict[str, int | str]] = []
    for archive in args.archives:
        output = args.out_dir / archive.name
        patched, missing, neutralized_relocs = patch_archive(
            archive, output, map_entries, oracle, oracle_phdrs, manifest
        )
        total_patched += patched
        total_missing += missing
        total_neutralized_relocs += neutralized_relocs
        print(
            f"{archive.name}: patched={patched} missing_map={missing} "
            f"neutralized_relocs={neutralized_relocs} -> {output}"
        )

    if total_patched == 0:
        print("error: no R_RISCV_48_LLUI relocations patched", file=sys.stderr)
        return 1
    if total_missing and not args.allow_missing_map:
        print(f"error: {total_missing} relocations had no map entry", file=sys.stderr)
        return 1
    if total_missing:
        print(f"warning: {total_missing} relocations had no map entry")
    if args.manifest:
        with args.manifest.open("w", encoding="utf-8") as f:
            for item in manifest:
                f.write(json.dumps(item, sort_keys=True, separators=(",", ":")))
                f.write("\n")
        print(f"patched relocation manifest: {args.manifest}")
    print(f"total patched R_RISCV_48_LLUI relocations: {total_patched}")
    print(f"total neutralized relocations: {total_neutralized_relocs}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
