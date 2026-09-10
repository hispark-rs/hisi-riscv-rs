#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Equal-layout negative: native RX initialization returns zero without work."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
from elftools.elf.elffile import ELFFile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def signed(value, bits):
    return value - (1 << bits) if value & (1 << (bits - 1)) else value


p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--elf", type=Path, required=True)
p.add_argument("--report", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--manifest", type=Path, required=True)
a = p.parse_args()
data = a.elf.read_bytes()
report = json.loads(a.report.read_bytes())
assert sha(data) == report["elf_sha256"] == "f60cf55a4cadc9c3bce563ac5297db2f4989f13b89c6296802338e64f0ebadc1"
edges = [e for e in report["edges"] if e["source"] == "__hisi_net0_rx_rebuild_probe"
         and e["target"] == "__hisi_net0_rom_init"]
assert len(edges) == 1
edge = edges[0]
with a.elf.open("rb") as stream:
    elf = ELFFile(stream)
    table = elf.get_section_by_name(".symtab")
    source = table.get_symbol_by_name(edge["source"])[0]
    target = table.get_symbol_by_name(edge["target"])[0]
    section = elf.get_section(source["st_shndx"])
    assert source["st_value"] <= edge["call_address"] < source["st_value"] + source["st_size"] - 7
    offset = section["sh_offset"] + edge["call_address"] - section["sh_addr"]
    assert offset == edge["file_offset"] and target["st_value"] == edge["target_address"]
first, second = struct.unpack_from("<II", data, offset)
assert first & 0xfff == 0x97 and second & 0xfffff == 0x80e7
resolved = (edge["call_address"] + signed(first & 0xfffff000, 32) + signed(second >> 20, 12)) & 0xfffffffe
assert resolved == target["st_value"]
replacement = struct.pack("<II", 0x513, 0x13)  # addi a0,zero,0; nop
changed = bytearray(data)
changed[offset:offset + 8] = replacement
assert changed[:offset] == data[:offset] and changed[offset + 8:] == data[offset + 8:]
a.output.write_bytes(changed)
a.manifest.write_text(json.dumps({
    "schema": "net0-rx-init-negative/v1", "source_commit": "5e7b179feadf9600aebc356f40d49748dd1ed5cd",
    "input_sha256": sha(data), "output_sha256": sha(changed),
    "file_offset": offset, "call_address": edge["call_address"],
    "old_bytes": data[offset:offset + 8].hex(), "new_bytes": replacement.hex(),
    "boundary": "One reviewed call replaced by zero-return/no-work; unchanged layout. Not real partial allocator failure.",
}, indent=2) + "\n")
print(json.dumps({"output_sha256": sha(changed), "call_address": edge["call_address"]}))
