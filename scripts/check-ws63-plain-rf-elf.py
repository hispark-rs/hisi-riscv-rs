#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Validate a stock-rust-lld WS63 radio ELF without external binutils."""

from __future__ import annotations

import argparse
import re
import struct
import tomllib
from pathlib import Path

from elftools.elf.elffile import ELFFile
from elftools.elf.relocation import RelocationSection
from elftools.elf.sections import SymbolTableSection


ROOT = Path(__file__).resolve().parents[1]
RADIO_PROFILES = (
    ROOT
    / "crates/chips/ws63/ws63-radio-sys/crates/hisi-rf-link/profiles"
)
VENDOR_RELOCATIONS = {58, 59, 61}
PATCH_VMA = 0x0014_C000
PATCH_SIZE = 0x928
PATCH_COMPARE_OFFSET = 0x610
PATCH_COUNT = 37


class ValidationError(RuntimeError):
    pass


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise ValidationError(message)


def defined_symbols(elf: ELFFile) -> dict[str, int]:
    symbols: dict[str, int] = {}
    for section in elf.iter_sections():
        if not isinstance(section, SymbolTableSection):
            continue
        for symbol in section.iter_symbols():
            if symbol.name and symbol["st_shndx"] != "SHN_UNDEF":
                symbols.setdefault(symbol.name, int(symbol["st_value"]))
    return symbols


def check_runtime_compatibility(symbols: set[str]) -> None:
    profile = tomllib.loads(
        (RADIO_PROFILES / "ws63-runtime-compat.toml").read_text()
    )
    patterns = [re.compile(pattern) for pattern in profile["namespace_patterns"]]
    provided = {
        entry["name"]
        for entry in profile["symbols"]
        if entry["classification"] == "provided"
    }
    off_path = {
        entry["name"]
        for entry in profile["symbols"]
        if entry["classification"] == "off-path"
    }
    actual = {name for name in symbols if any(pattern.search(name) for pattern in patterns)}
    expect(
        actual == provided,
        "runtime compatibility drift: "
        f"missing={sorted(provided - actual)}, "
        f"unexpected={sorted(actual - provided)}, "
        f"off_path={sorted(actual & off_path)}",
    )


def check_upstream_supplicant(symbols: set[str]) -> None:
    profile = tomllib.loads(
        (RADIO_PROFILES / "ws63-supplicant-boundary.toml").read_text()
    )
    required = {
        "eloop_init",
        "hisi_wpa_create",
        "hisi_wpa_driver_install",
        "hisi_wpa_poll",
        "wpa_supplicant_init",
    }
    missing = required - symbols
    expect(not missing, f"upstream supplicant closure is missing {sorted(missing)}")
    legacy = symbols & set(profile["legacy_provider_symbols"])
    expect(not legacy, f"legacy supplicant provider symbols are reachable: {sorted(legacy)}")


def check_elf(path: Path, require_upstream_supplicant: bool) -> None:
    with path.open("rb") as stream:
        elf = ELFFile(stream)
        expect(elf.elfclass == 32, "firmware is not ELF32")
        expect(elf.little_endian, "firmware is not little-endian")
        expect(elf["e_machine"] == "EM_RISCV", "firmware is not RISC-V")

        expected_sections = {
            ".boot_header": (0x0023_0000, 0x300),
            ".wifi_pkt_ram": (0x00A0_0000, 0xC000),
            ".patch": (PATCH_VMA, PATCH_SIZE),
        }
        for name, (address, size) in expected_sections.items():
            section = elf.get_section_by_name(name)
            expect(section is not None, f"missing required section {name}")
            expect(int(section["sh_addr"]) == address, f"{name} address drift")
            expect(int(section["sh_size"]) == size, f"{name} size drift")
        packet_ram = elf.get_section_by_name(".wifi_pkt_ram")
        expect(packet_ram["sh_type"] == "SHT_NOBITS", ".wifi_pkt_ram must be NOLOAD")

        patch = elf.get_section_by_name(".patch")
        patch_data = patch.data()
        expect(len(patch_data) == PATCH_SIZE, ".patch payload is truncated")
        compare_vma, count = struct.unpack_from(
            "<II", patch_data, PATCH_COMPARE_OFFSET + 4
        )
        expect(compare_vma == PATCH_VMA, "ROM patch compare header VMA drift")
        expect(count == PATCH_COUNT, f"ROM patch count drift: {count}")
        originals = struct.unpack_from(
            f"<{PATCH_COUNT}I", patch_data, PATCH_COMPARE_OFFSET + 12
        )
        expect(all(address & 1 for address in originals), "ROM patch compare address lost Thumb/vendor tag")
        expect(len(set(originals)) == PATCH_COUNT, "duplicate ROM patch compare address")
        for index in range(PATCH_COUNT):
            offset = 16 + index * 8
            auipc, jalr = struct.unpack_from("<II", patch_data, offset)
            expect(auipc & 0x7F == 0x17, f"ROM patch {index} is not AUIPC")
            expect(jalr & 0x7F == 0x67, f"ROM patch {index} is not JALR")

        for section in elf.iter_sections():
            if not isinstance(section, RelocationSection):
                continue
            for relocation in section.iter_relocations():
                kind = int(relocation["r_info_type"])
                expect(
                    kind not in VENDOR_RELOCATIONS,
                    f"vendor relocation {kind} remains in {section.name}",
                )

        values = defined_symbols(elf)
        expected_symbols = {
            "__hisi_ws63_rom_patch_table": PATCH_VMA,
            "__rom_patch_begin__": PATCH_VMA,
            "__rom_patch_cmp_begin__": PATCH_VMA + PATCH_COMPARE_OFFSET,
            "__rom_patch_end__": PATCH_VMA + PATCH_SIZE,
        }
        for name, value in expected_symbols.items():
            expect(values.get(name) == value, f"{name} address drift")
        symbol_names = set(values)
        check_runtime_compatibility(symbol_names)
        if require_upstream_supplicant:
            check_upstream_supplicant(symbol_names)

    print(
        "WS63 plain Cargo RF ELF OK: "
        f"path={path}, rom_patches={PATCH_COUNT}, vendor_relocations=0, "
        f"upstream_supplicant={int(require_upstream_supplicant)}"
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--require-upstream-supplicant", action="store_true")
    args = parser.parse_args()
    if not args.elf.is_file():
        raise ValidationError(f"ELF does not exist: {args.elf}")
    check_elf(args.elf, args.require_upstream_supplicant)


if __name__ == "__main__":
    try:
        main()
    except ValidationError as error:
        raise SystemExit(f"WS63 plain Cargo RF ELF invalid: {error}") from error
