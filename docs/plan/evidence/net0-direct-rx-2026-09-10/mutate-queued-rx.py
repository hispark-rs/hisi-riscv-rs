#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Maintainer negative ELF: force the genuine message-595 producer branch."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
from elftools.elf.elffile import ELFFile

p = argparse.ArgumentParser()
p.add_argument("--input", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--manifest", type=Path, required=True)
a = p.parse_args()
original = a.input.read_bytes()
sha = lambda data: hashlib.sha256(data).hexdigest()
if sha(original) != "9f1e8d4341eb53c321b3793c391a94bb26b77856887a4c77a82e30732b109a1c":
    raise SystemExit("Unexpected reviewed input ELF")
with a.input.open("rb") as stream:
    elf = ELFFile(stream)
    symbols = elf.get_section_by_name(".symtab").get_symbol_by_name("hmac_rx_data_event_adapt")
    if len(symbols) != 1 or symbols[0]["st_size"] != 144:
        raise SystemExit("RX producer layout changed")
    symbol = symbols[0]
    section = elf.get_section(symbol["st_shndx"])
    address = symbol["st_value"] + 80
    offset = section["sh_offset"] + address - section["sh_addr"]
if original[offset:offset + 2] != bytes.fromhex("01ed"):
    raise SystemExit("Expected c.bnez a0,+24 after throughput-18 query")
# Both branches resolve to producer+104. No address, section or other byte moves.
changed = bytearray(original)
struct.pack_into("<H", changed, offset, 0xa821)  # c.j +24
if sum(x != y for x, y in zip(original, changed)) != 2:
    raise SystemExit("Mutation must change exactly the two branch bytes")
a.output.write_bytes(changed)
a.manifest.write_text(json.dumps({
    "schema": "net0-queued-rx-negative/v1", "source_commit": "9afac3366e1a75cb7f734266a0d4a0322462716e",
    "input_sha256": sha(original), "output_sha256": sha(changed),
    "function": "hmac_rx_data_event_adapt", "instruction_offset": 80,
    "address": address, "file_offset": offset, "before": "01ed", "after": "21a8",
    "target_offset": 104, "expected": "Native producer rejected, sticky flag=1, no successful L2 payload",
    "boundary": "Intentional test-only control-flow mutation; never a release artifact or connectivity success",
}, indent=2) + "\n")
print(json.dumps({"mutated_bytes": 2, "address": hex(address), "output_sha256": sha(changed)}))
