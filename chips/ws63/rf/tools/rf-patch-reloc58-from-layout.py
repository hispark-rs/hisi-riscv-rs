#!/usr/bin/env python3
"""Patch HiSilicon R_RISCV_48_LLUI relocations from a final rust-lld layout.

This is the safer successor to the vendor-oracle patch lane. The flow is:

1. `--mode neutralize`: copy RF archives, turn vendor-only relocations into
   R_RISCV_NONE, and leave instruction bytes unchanged. This lets upstream
   rust-lld produce a final ELF + map whose section layout is authoritative.
2. `--mode patch`: copy the original RF archives again and patch each
   R_RISCV_48_LLUI immediate to the address computed from the rust-lld final
   layout map plus the relocation symbol/addend. Then neutralize that relocation
   so the second rust-lld link consumes ordinary ELF objects.

The vendor linker is not used by this path.
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
R_RISCV_BRANCHI = 59
R_RISCV_LLUI_REP = 61
SHT_SYMTAB = 2
SHT_RELA = 4
SHN_UNDEF = 0


@dataclass(frozen=True)
class Section:
    name: str
    sh_type: int
    flags: int
    addr: int
    offset: int
    size: int
    entsize: int
    link: int
    info: int


@dataclass(frozen=True)
class Symbol:
    name: str
    value: int
    size: int
    info: int
    other: int
    shndx: int


def u16(buf: bytes | bytearray, off: int) -> int:
    return struct.unpack_from("<H", buf, off)[0]


def u32(buf: bytes | bytearray, off: int) -> int:
    return struct.unpack_from("<I", buf, off)[0]


def i32(buf: bytes | bytearray, off: int) -> int:
    return struct.unpack_from("<i", buf, off)[0]


def put_u32(buf: bytearray, off: int, value: int) -> None:
    struct.pack_into("<I", buf, off, value & 0xFFFF_FFFF)


def encode_branchi(word: int, offset: int) -> int:
    """Encode the signed, halfword-aligned LinxCore BRANCHI displacement."""
    if offset & 1:
        raise ValueError(f"R_RISCV_BRANCHI offset is not halfword aligned: {offset}")
    if not -0x200 <= offset <= 0x1FE:
        raise ValueError(f"R_RISCV_BRANCHI offset out of range: {offset}")

    encoded = offset & 0x3FF
    immediate_mask = 0x00F0_0F80
    immediate = ((encoded & 0x03E) << 6) | ((encoded & 0x3C0) << 14)
    return (word & ~immediate_mask) | immediate


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
                u32(buf, off + 8),
                u32(buf, off + 12),
                u32(buf, off + 16),
                u32(buf, off + 20),
                u32(buf, off + 36),
                u32(buf, off + 24),
                u32(buf, off + 28),
            )
        )

    shstr = b""
    if shstrndx < len(raw):
        _, _, _, _, off, size, _, _, _ = raw[shstrndx]
        shstr = buf[off : off + size]

    sections = []
    for name_off, typ, flags, addr, off, size, entsize, link, info in raw:
        sections.append(Section(cstr(shstr, name_off), typ, flags, addr, off, size, entsize, link, info))
    return sections


def parse_symtabs(buf: bytes | bytearray, sections: list[Section]) -> dict[int, list[Symbol]]:
    symtabs: dict[int, list[Symbol]] = {}
    for sec_idx, sec in enumerate(sections):
        if sec.sh_type != SHT_SYMTAB or sec.entsize == 0 or sec.link >= len(sections):
            continue
        strtab_sec = sections[sec.link]
        strtab = buf[strtab_sec.offset : strtab_sec.offset + strtab_sec.size]
        symbols = []
        for off in range(sec.offset, sec.offset + sec.size, sec.entsize):
            name_off = u32(buf, off + 0)
            symbols.append(
                Symbol(
                    name=cstr(strtab, name_off) if name_off else "",
                    value=u32(buf, off + 4),
                    size=u32(buf, off + 8),
                    info=buf[off + 12],
                    other=buf[off + 13],
                    shndx=u16(buf, off + 14),
                )
            )
        symtabs[sec_idx] = symbols
    return symtabs


def parse_final_symbols(path: Path) -> dict[str, int]:
    buf = path.read_bytes()
    sections = parse_sections(buf, str(path))
    symbols: dict[str, int] = {}
    for symtab in parse_symtabs(buf, sections).values():
        for sym in symtab:
            if sym.name and sym.shndx != SHN_UNDEF and sym.value:
                symbols.setdefault(sym.name, sym.value)
    return symbols


def parse_final_alloc_sections(path: Path) -> list[tuple[str, int, bytes]]:
    buf = path.read_bytes()
    sections = parse_sections(buf, str(path))
    out = []
    for sec in sections:
        if sec.flags & 0x2 and sec.size and sec.sh_type != 8:
            out.append((sec.name, sec.addr, bytes(buf[sec.offset : sec.offset + sec.size])))
    return out


def c_string_at(buf: bytes | bytearray, off: int) -> bytes | None:
    if off >= len(buf):
        return None
    end = buf.find(b"\0", off)
    if end < 0:
        return None
    return bytes(buf[off : end + 1])


def find_alloc_bytes(final_alloc: list[tuple[str, int, bytes]], needle: bytes) -> int | None:
    if not needle:
        return None
    found: int | None = None
    for _name, addr, data in final_alloc:
        pos = data.find(needle)
        if pos < 0:
            continue
        candidate = addr + pos
        if found is None or candidate < found:
            found = candidate
    return found


def parse_lld_map(path: Path) -> dict[tuple[str, str, str], int]:
    entries: dict[tuple[str, str, str], int] = {}
    lld_re = re.compile(
        r"^\s*([0-9a-fA-F]+)\s+"
        r"([0-9a-fA-F]+)\s+"
        r"([0-9a-fA-F]+)\s+"
        r"\d+\s+"
        r"(\S+\.a)\(([^)]+)\):\(([^)]+)\)"
    )
    with path.open("r", encoding="utf-8", errors="replace") as f:
        for line in f:
            match = lld_re.search(line)
            if not match:
                continue
            vma = int(match.group(1), 16)
            size = int(match.group(3), 16)
            if vma == 0 or size == 0:
                continue
            archive = os.path.basename(match.group(4))
            member = match.group(5)
            section = match.group(6)
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
        yield name, data_start, data_end
        pos = data_end + (size & 1)


def local_symbol_addr(
    archive: str,
    member: str,
    sections: list[Section],
    symbol: Symbol,
    map_entries: dict[tuple[str, str, str], int],
) -> int | None:
    if symbol.shndx == SHN_UNDEF or symbol.shndx >= len(sections):
        return None
    sec = sections[symbol.shndx]
    base = map_entries.get((archive, member, sec.name))
    if base is None:
        return None
    return base + symbol.value


def merged_string_symbol_addr(
    sections: list[Section],
    symbol: Symbol,
    obj: bytes | bytearray,
    final_alloc: list[tuple[str, int, bytes]],
) -> int | None:
    if symbol.shndx >= len(sections):
        return None
    sec = sections[symbol.shndx]
    if ".rodata" not in sec.name or "str" not in sec.name:
        return None
    needle = c_string_at(obj[sec.offset : sec.offset + sec.size], symbol.value)
    if needle is None:
        return None
    return find_alloc_bytes(final_alloc, needle)


def describe_symbol(sections: list[Section], symbol: Symbol) -> str:
    if symbol.shndx == SHN_UNDEF:
        sec = "UND"
    elif symbol.shndx < len(sections):
        sec = sections[symbol.shndx].name
    else:
        sec = f"SHN_{symbol.shndx:x}"
    return f"{symbol.name or '<anon>'}@{sec}+0x{symbol.value:x}"


def patch_object(
    obj: bytearray,
    archive: str,
    member: str,
    mode: str,
    map_entries: dict[tuple[str, str, str], int],
    final_symbols: dict[str, int],
    final_alloc: list[tuple[str, int, bytes]],
    manifest: list[dict[str, int | str]],
    diagnostics: list[str],
) -> tuple[int, int, int, int]:
    sections = parse_sections(obj, f"{archive}({member})")
    symtabs = parse_symtabs(obj, sections)
    patched = 0
    neutralized = 0
    missing_layout = 0
    unresolved = 0

    for relsec in sections:
        if relsec.sh_type != SHT_RELA or relsec.info >= len(sections):
            continue
        target = sections[relsec.info]
        if not target.name:
            continue
        entsize = relsec.entsize or 12
        symtab = symtabs.get(relsec.link, [])
        for idx in range(relsec.size // entsize):
            ent = relsec.offset + idx * entsize
            r_offset = u32(obj, ent + 0)
            r_info = u32(obj, ent + 4)
            r_type = r_info & 0xFF
            r_sym = r_info >> 8
            addend = i32(obj, ent + 8) if entsize >= 12 else 0

            if target.name.startswith(".debug") and r_type != R_RISCV_NONE:
                put_u32(obj, ent + 4, (r_info & ~0xFF) | R_RISCV_NONE)
                neutralized += 1
                continue

            if r_type == R_RISCV_LLUI_REP:
                put_u32(obj, ent + 4, (r_info & ~0xFF) | R_RISCV_NONE)
                neutralized += 1
                continue

            if r_type == R_RISCV_BRANCHI:
                if mode == "patch":
                    section_vma = map_entries.get((archive, member, target.name))
                    if section_vma is None:
                        missing_layout += 1
                        if len(diagnostics) < 80:
                            diagnostics.append(
                                f"missing_layout {archive}({member}) {target.name}+0x{r_offset:x}"
                            )
                        continue
                    if r_sym >= len(symtab):
                        unresolved += 1
                        if len(diagnostics) < 80:
                            diagnostics.append(
                                f"bad_sym_index {archive}({member}) {target.name}+0x{r_offset:x} "
                                f"sym={r_sym} symtab_len={len(symtab)}"
                            )
                        continue
                    sym = symtab[r_sym]
                    addr = local_symbol_addr(archive, member, sections, sym, map_entries)
                    if addr is None and sym.name:
                        addr = final_symbols.get(sym.name)
                    if addr is None:
                        unresolved += 1
                        if len(diagnostics) < 80:
                            diagnostics.append(
                                f"unresolved {archive}({member}) {target.name}+0x{r_offset:x} "
                                f"symbol={describe_symbol(sections, sym)} addend={addend}"
                            )
                        continue
                    source = section_vma + r_offset
                    branch_offset = addr + addend - source
                    instruction_offset = target.offset + r_offset
                    try:
                        instruction = encode_branchi(u32(obj, instruction_offset), branch_offset)
                    except ValueError as error:
                        unresolved += 1
                        if len(diagnostics) < 80:
                            diagnostics.append(
                                f"unresolved {archive}({member}) {target.name}+0x{r_offset:x} "
                                f"symbol={describe_symbol(sections, sym)}: {error}"
                            )
                        continue
                    put_u32(obj, instruction_offset, instruction)
                    manifest.append(
                        {
                            "relocation": "R_RISCV_BRANCHI",
                            "archive": archive,
                            "member": member,
                            "section": target.name,
                            "section_vma": section_vma,
                            "rel_offset": r_offset,
                            "rel_vma": source,
                            "symbol": sym.name,
                            "symbol_shndx": sym.shndx,
                            "symbol_value": sym.value,
                            "addend": addend,
                            "branch_offset": branch_offset,
                            "instruction": instruction,
                        }
                    )
                    patched += 1

                put_u32(obj, ent + 4, (r_info & ~0xFF) | R_RISCV_NONE)
                neutralized += 1
                continue

            if r_type != R_RISCV_48_LLUI:
                continue

            if mode == "patch":
                section_vma = map_entries.get((archive, member, target.name))
                if section_vma is None:
                    missing_layout += 1
                    if len(diagnostics) < 80:
                        diagnostics.append(
                            f"missing_layout {archive}({member}) {target.name}+0x{r_offset:x}"
                        )
                    continue
                if r_sym >= len(symtab):
                    unresolved += 1
                    if len(diagnostics) < 80:
                        diagnostics.append(
                            f"bad_sym_index {archive}({member}) {target.name}+0x{r_offset:x} "
                            f"sym={r_sym} symtab_len={len(symtab)}"
                        )
                    continue
                sym = symtab[r_sym]
                addr = local_symbol_addr(archive, member, sections, sym, map_entries)
                if addr is None:
                    addr = merged_string_symbol_addr(sections, sym, obj, final_alloc)
                if addr is None and sym.name:
                    addr = final_symbols.get(sym.name)
                if addr is None:
                    unresolved += 1
                    if len(diagnostics) < 80:
                        diagnostics.append(
                            f"unresolved {archive}({member}) {target.name}+0x{r_offset:x} "
                            f"symbol={describe_symbol(sections, sym)} addend={addend}"
                        )
                    continue
                imm32 = (addr + addend) & 0xFFFF_FFFF
                obj[target.offset + r_offset + 2 : target.offset + r_offset + 6] = imm32.to_bytes(
                    4, "little"
                )
                manifest.append(
                    {
                        "archive": archive,
                        "relocation": "R_RISCV_48_LLUI",
                        "member": member,
                        "section": target.name,
                        "section_vma": section_vma,
                        "rel_offset": r_offset,
                        "rel_vma": section_vma + r_offset,
                        "symbol": sym.name,
                        "symbol_shndx": sym.shndx,
                        "symbol_value": sym.value,
                        "addend": addend,
                        "imm32": imm32,
                    }
                )
                patched += 1

            put_u32(obj, ent + 4, (r_info & ~0xFF) | R_RISCV_NONE)
            neutralized += 1

    return patched, neutralized, missing_layout, unresolved


def patch_archive(
    archive: Path,
    output: Path,
    mode: str,
    map_entries: dict[tuple[str, str, str], int],
    final_symbols: dict[str, int],
    final_alloc: list[tuple[str, int, bytes]],
    manifest: list[dict[str, int | str]],
    diagnostics: list[str],
) -> tuple[int, int, int, int]:
    shutil.copyfile(archive, output)
    buf = bytearray(output.read_bytes())
    total = (0, 0, 0, 0)
    for member, data_start, data_end in iter_ar_members(buf):
        data = buf[data_start:data_end]
        if not data.startswith(ELF_MAGIC):
            continue
        obj = bytearray(data)
        counts = patch_object(
            obj,
            archive.name,
            member,
            mode,
            map_entries,
            final_symbols,
            final_alloc,
            manifest,
            diagnostics,
        )
        if any(counts):
            buf[data_start:data_end] = obj
            total = tuple(a + b for a, b in zip(total, counts))
    output.write_bytes(buf)
    return total


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=["neutralize", "patch"], required=True)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--final-map", type=Path)
    parser.add_argument("--final-elf", type=Path)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument(
        "--allow-missing-layout",
        action="store_true",
        help=(
            "Allow relocation-58 sites whose input section was not pulled into "
            "the rust-lld layout pass. These are off-path for the selected root."
        ),
    )
    parser.add_argument("archives", nargs="+", type=Path)
    args = parser.parse_args()

    if args.mode == "patch" and (args.final_map is None or args.final_elf is None):
        parser.error("--mode patch requires --final-map and --final-elf")

    map_entries = parse_lld_map(args.final_map) if args.final_map else {}
    final_symbols = parse_final_symbols(args.final_elf) if args.final_elf else {}
    final_alloc = parse_final_alloc_sections(args.final_elf) if args.final_elf else []
    args.out_dir.mkdir(parents=True, exist_ok=True)

    totals = (0, 0, 0, 0)
    manifest: list[dict[str, int | str]] = []
    diagnostics: list[str] = []
    for archive in args.archives:
        output = args.out_dir / archive.name
        counts = patch_archive(
            archive,
            output,
            args.mode,
            map_entries,
            final_symbols,
            final_alloc,
            manifest,
            diagnostics,
        )
        totals = tuple(a + b for a, b in zip(totals, counts))
        patched, neutralized, missing_layout, unresolved = counts
        print(
            f"{archive.name}: patched={patched} neutralized={neutralized} "
            f"missing_layout={missing_layout} unresolved={unresolved} -> {output}"
        )

    patched, neutralized, missing_layout, unresolved = totals
    if args.mode == "patch":
        if patched == 0:
            print("error: no R_RISCV_48_LLUI relocations patched", file=sys.stderr)
            return 1
        if missing_layout and not args.allow_missing_layout:
            print(
                f"error: missing_layout={missing_layout} unresolved={unresolved}",
                file=sys.stderr,
            )
            for item in diagnostics:
                print(f"  {item}", file=sys.stderr)
            return 1
        if unresolved:
            print(
                f"error: unresolved={unresolved} missing_layout={missing_layout}",
                file=sys.stderr,
            )
            for item in diagnostics:
                if item.startswith("unresolved") or item.startswith("bad_sym_index"):
                    print(f"  {item}", file=sys.stderr)
            return 1
        if missing_layout:
            print(f"warning: ignored off-path missing_layout={missing_layout}")
        if args.manifest:
            with args.manifest.open("w", encoding="utf-8") as f:
                for item in manifest:
                    f.write(json.dumps(item, sort_keys=True, separators=(",", ":")))
                    f.write("\n")
            print(f"patched relocation manifest: {args.manifest}")
    elif neutralized == 0:
        print("error: no vendor relocations neutralized", file=sys.stderr)
        return 1

    print(f"total patched={patched} neutralized={neutralized}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
