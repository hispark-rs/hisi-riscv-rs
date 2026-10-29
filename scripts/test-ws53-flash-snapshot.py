#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Test Flash ELF snapshot consistency, including the optional RAM audit."""
import argparse
import hashlib
import io
from pathlib import Path
import runpy
import struct
from types import SimpleNamespace
from elftools.elf.elffile import ELFFile


class ReplacedPath:
    """Simulate an input being replaced between separate file reads."""

    def __init__(self, image):
        self.image = image
        self.replaced = False
        self.reads = 0

    def read_bytes(self):
        self.reads += 1
        return b"different ELF" if self.replaced or self.reads > 1 else self.image

    def open(self, mode):
        if mode != "rb":
            raise ValueError("unexpected file mode")
        self.replaced = True
        return io.BytesIO(self.image)

    def stat(self):
        return SimpleNamespace(st_size=len(self.image))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--ram-functions-elf", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    validate = runpy.run_path(str(root / "scripts/check-ws53-flash-elf.py"))["validate"]
    audit = runpy.run_path(str(root / "crates/hisi-riscv-rt/scripts/check-ws53-ram-functions.py"))["validate_bytes"]
    for path, ram in ((args.elf, False), (args.ram_functions_elf, True)):
        image = path.read_bytes()
        digest = hashlib.sha256(image).hexdigest()
        changing = ReplacedPath(image)
        report, payload = validate(changing, allow_ram_functions=ram)
        if changing.reads != 1 or report["elf_sha256"] != digest:
            raise RuntimeError("Flash validation did not use one image snapshot")
        if report["payload_sha256"] != hashlib.sha256(payload).hexdigest():
            raise RuntimeError("payload does not match report")
        if ram and (report["ram_functions"]["elf_sha256"] != digest or audit(image)["elf_sha256"] != digest):
            raise RuntimeError("RAM audit used a different image snapshot")
        corrupt = bytearray(image)
        corrupt[24:28] = (0x28000).to_bytes(4, "little")
        try:
            validate(ReplacedPath(bytes(corrupt)), allow_ram_functions=ram)
        except ValueError:
            pass
        else:
            raise RuntimeError("invalid Flash entry accepted")
        elf = ELFFile(io.BytesIO(image))
        section_index = next(i for i, s in enumerate(elf.iter_sections())
                             if s["sh_flags"] & 4 and s.name != ".ws53.ramtext")
        section = elf.get_section(section_index)
        header = elf["e_shoff"] + section_index * elf["e_shentsize"]
        mutant = bytearray(image)
        copy = bytes(section.data())
        struct.pack_into("<I", mutant, header + 16, len(mutant))
        struct.pack_into("<I", mutant, section["sh_offset"], 0x1000202F)
        mutant.extend(copy)
        try:
            validate(ReplacedPath(bytes(mutant)), allow_ram_functions=ram)
        except ValueError as error:
            if "executable bytes outside file-backed Flash" not in str(error):
                raise
        else:
            raise RuntimeError("aliased Flash section accepted despite unsafe loaded code")
        print(f"PASS snapshot ram_functions={ram}")


if __name__ == "__main__":
    main()
