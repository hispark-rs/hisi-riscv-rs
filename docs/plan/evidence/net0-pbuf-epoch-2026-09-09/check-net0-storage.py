#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Compare a target-built NET0 resource descriptor with its final ELF storage."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest

from elftools.elf.elffile import ELFFile


def validate(words, control_size, shared_size, packet_size, rf_size, runtime_size):
    if len(words) != 16 or words[:2] != [int.from_bytes(b"NET0", "little"), 3]:
        raise ValueError("missing or unknown NET0 target layout schema")
    keys = ("control_bytes", "l2_offset", "l2_bytes", "payload_bytes", "metadata_bytes",
            "rx_slots", "tx_slots", "mtu", "shared_arena_bytes", "main_stack_bytes",
            "packet_ram_bytes", "rf_arena_bytes", "runtime_arena_bytes", "native_pbuf_prefix_bytes")
    report = dict(zip(keys, words[2:]))
    if report["control_bytes"] != control_size:
        raise ValueError("target report differs from actual NET0_CONTROL symbol size")
    if report["l2_offset"] + report["l2_bytes"] > control_size:
        raise ValueError("L2 allocation extends outside caller-owned control storage")
    if (report["rx_slots"], report["tx_slots"], report["mtu"]) != (4, 4, 1514):
        raise ValueError("unexpected NET0 queue shape; review profile change")
    if report["payload_bytes"] != (report["rx_slots"] + report["tx_slots"]) * report["mtu"]:
        raise ValueError("payload bytes do not match packet slot storage")
    if report["metadata_bytes"] <= 0 or report["payload_bytes"] + report["metadata_bytes"] != report["l2_bytes"]:
        raise ValueError("L2 payload/metadata sum does not match physical storage")
    if report["shared_arena_bytes"] != shared_size:
        raise ValueError(f"target report differs from linked shared arenas: report={report['shared_arena_bytes']}, linked={shared_size}")
    if (report["rf_arena_bytes"], report["runtime_arena_bytes"]) != (rf_size, runtime_size):
        raise ValueError("RF/runtime child budget differs from its physical arena")
    if rf_size <= 0 or runtime_size <= 0 or rf_size + runtime_size != shared_size:
        raise ValueError("shared arena total must equal the physical child allocations")
    if report["packet_ram_bytes"] != packet_size or packet_size != 0xc000:
        raise ValueError("radio packet RAM was changed or misreported")
    if report["main_stack_bytes"] != 0x8000:
        raise ValueError("existing 32 KiB main stack contract was changed")
    if report["native_pbuf_prefix_bytes"] != 16:
        raise ValueError("native pbuf metadata cost changed; review RF heap budget")
    return report


def disjoint(regions):
    ordered = sorted((start, start + size, name) for name, (start, size) in regions.items())
    for start, end, name in ordered:
        if start < 0 or start >= end or end > 1 << 32:
            raise ValueError(f"invalid RV32 memory range: {name}")
    for (_, end, previous), (start, _, name) in zip(ordered, ordered[1:]):
        if end > start:
            raise ValueError(f"physical storage overlaps: {previous}, {name}")


def inspect(path):
    with path.open("rb") as stream:
        elf = ELFFile(stream)
        if elf.elfclass != 32 or not elf.little_endian or elf["e_machine"] != "EM_RISCV":
            raise ValueError("expected the final RV32 little-endian ELF, not a host report")
        symbols = elf.get_section_by_name(".symtab")
        def symbol(name):
            matches = symbols.get_symbol_by_name(name) if symbols else None
            if not matches or len(matches) != 1:
                raise ValueError(f"missing or ambiguous ELF symbol: {name}")
            return matches[0]
        control = symbol("NET0_CONTROL")
        rf = symbol("NET0_RF_ARENA")
        runtime = symbol("NET0_RTOS_ARENA")
        layout = symbol("NET0_STORAGE_LAYOUT")
        if layout["st_size"] != 64 or not isinstance(layout["st_shndx"], int):
            raise ValueError("invalid NET0 layout byte length")
        section = elf.get_section(layout["st_shndx"])
        offset = layout["st_value"] - section["sh_addr"]
        if section["sh_type"] == "SHT_NOBITS" or offset < 0 or offset + 64 > section["sh_size"]:
            raise ValueError("layout must be initialized bytes within its ELF section")
        words = list(struct.unpack("<16I", section.data()[offset:offset + 64]))
        shared = elf.get_section_by_name(".hisi_shared_arenas")
        packet = elf.get_section_by_name(".wifi_pkt_ram")
        if shared is None or packet is None:
            raise ValueError("missing shared arena or Wi-Fi packet section")
        report = validate(words, control["st_size"], shared["sh_size"], packet["sh_size"],
                          rf["st_size"], runtime["st_size"])
        stack_start = symbol("__stack_start__")["st_value"]
        stack_top = symbol("__stack_top__")["st_value"]
        if stack_top - stack_start != report["main_stack_bytes"]:
            raise ValueError("main stack report differs from actual linker symbols")
        if shared["sh_addr"] + shared["sh_size"] > stack_start:
            raise ValueError("shared arenas overlap the main stack")
        for arena in (rf, runtime):
            if not isinstance(arena["st_shndx"], int):
                raise ValueError("arena must be a physical object, not an absolute symbol")
            arena_section = elf.get_section(arena["st_shndx"])
            if (arena_section.name != ".hisi_shared_arenas"
                    or arena_section["sh_type"] != "SHT_NOBITS"
                    or arena_section["sh_flags"] & 3 != 3):
                raise ValueError("arena must be writable allocated NOLOAD shared storage")
            if (arena["st_value"] < shared["sh_addr"]
                    or arena["st_value"] + arena["st_size"] > shared["sh_addr"] + shared["sh_size"]):
                raise ValueError("arena is not contained in its physical section")
        if not isinstance(control["st_shndx"], int):
            raise ValueError("control storage must be a physical object")
        control_section = elf.get_section(control["st_shndx"])
        if not control_section["sh_flags"] & 1 or not control_section["sh_flags"] & 2:
            raise ValueError("control storage is not writable allocated target memory")
        begin = control["st_value"]
        end = begin + control["st_size"]
        if begin < control_section["sh_addr"] or end > control_section["sh_addr"] + control_section["sh_size"]:
            raise ValueError("control object is not contained in its linked section")
        disjoint({"control": (begin, control["st_size"]),
                  "rf_arena": (rf["st_value"], rf["st_size"]),
                  "runtime_arena": (runtime["st_value"], runtime["st_size"]),
                  "main_stack": (stack_start, stack_top - stack_start),
                  "packet_ram": (packet["sh_addr"], packet["sh_size"])})
        report["control_address"] = begin
        report["rf_arena_address"] = rf["st_value"]
        report["runtime_arena_address"] = runtime["st_value"]
        report["main_stack_address"] = stack_start
        report["packet_ram_address"] = packet["sh_addr"]
    report.update(schema="net0-linked-storage/v3", status="pass",
                  elf_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                  boundary="Physical storage/link validation only; no native fence or HIL claim")
    return report


class ContractTests(unittest.TestCase):
    def test_rejects_size_sum_and_capacity_drift(self):
        words = [int.from_bytes(b"NET0", "little"), 3, 22000, 2304, 12736,
                 12112, 624, 4, 4, 1514, 299072, 32768, 49152, 101952, 197120, 16]
        validate(words, 22000, 299072, 49152, 101952, 197120)
        for index in range(len(words)):
            wrong = words.copy()
            wrong[index] += 1
            if index == 3:
                wrong[index] = 22000
            with self.subTest(index=index), self.assertRaises(ValueError):
                validate(wrong, 22000, 299072, 49152, 101952, 197120)

    def test_rejects_child_budget_swap_even_when_total_matches(self):
        words = [int.from_bytes(b"NET0", "little"), 3, 22000, 2304, 12736,
                 12112, 624, 4, 4, 1514, 299072, 32768, 49152, 101952, 197120, 16]
        with self.assertRaisesRegex(ValueError, "child budget"):
            validate(words, 22000, 299072, 49152, 197120, 101952)

    def test_range_ownership(self):
        disjoint({"a": (100, 16), "b": (116, 16)})
        for start, size in ((115, 16), (100, 16), (116, 0), ((1 << 32) - 1, 2)):
            with self.subTest(start=start, size=size), self.assertRaises(ValueError):
                disjoint({"a": (100, 16), "b": (start, size)})


def check_elf_tampering(path):
    """Exercise the actual ELF reader, including symbol and section ownership."""
    inspect(path)
    with path.open("rb") as stream:
        elf = ELFFile(stream)
        table = elf.get_section_by_name(".symtab")
        found = {sym.name: (index, sym) for index, sym in enumerate(table.iter_symbols())}
        def field(name, offset):
            return table["sh_offset"] + found[name][0] * table["sh_entsize"] + offset
        layout = found["NET0_STORAGE_LAYOUT"][1]
        section = elf.get_section(layout["st_shndx"])
        report_offset = section["sh_offset"] + layout["st_value"] - section["sh_addr"]
        rf_address = found["NET0_RF_ARENA"][1]["st_value"]
        mutations = {
            "control_size": (field("NET0_CONTROL", 8), "<I", 1),
            "missing_layout_name": (field("NET0_STORAGE_LAYOUT", 0), "<I", 0),
            "layout_outside_section": (field("NET0_STORAGE_LAYOUT", 4), "<I", 0),
            "wrong_schema": (report_offset + 4, "<I", 0),
            "hidden_pbuf_metadata": (report_offset + 60, "<I", 0),
            "runtime_aliases_rf": (field("NET0_RTOS_ARENA", 4), "<I", rf_address),
            "absolute_arena": (field("NET0_RF_ARENA", 14), "<H", 0xfff1),
            "stack_size": (field("__stack_top__", 4), "<I", 0),
        }
    original = path.read_bytes()
    with tempfile.TemporaryDirectory(prefix="net0-elf-check-") as temporary:
        candidate = Path(temporary) / "mutated.elf"
        for name, (offset, encoding, value) in mutations.items():
            data = bytearray(original)
            struct.pack_into(encoding, data, offset, value)
            candidate.write_bytes(data)
            try:
                inspect(candidate)
            except ValueError:
                continue
            raise ValueError(f"ELF mutation was not rejected: {name}")
    print(f"NET0 ELF tamper tests: {len(mutations)}/{len(mutations)} rejected")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path, nargs="?")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(ContractTests))
        if not result.wasSuccessful():
            raise SystemExit(1)
    if args.elf:
        if args.self_test:
            check_elf_tampering(args.elf)
        data = json.dumps(inspect(args.elf), sort_keys=True, indent=2) + "\n"
        if args.output:
            args.output.write_text(data)
        print(data, end="")
    elif not args.self_test:
        parser.error("provide an ELF or --self-test")


if __name__ == "__main__":
    main()
