#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyelftools==0.32"]
# ///
"""Mutate a linked WS53 smoke ELF to exercise rejection before hardware loading."""
import argparse
import hashlib
import importlib.util
import io
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
from types import SimpleNamespace
from elftools.elf.elffile import ELFFile

CHECK = Path(__file__).with_name("check-ws53-ram-elf.py")
spec = importlib.util.spec_from_file_location("ws53_elf", CHECK)
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("elf", type=Path)
    args = parser.parse_args()
    validator.validate(args.elf)
    original = args.elf.read_bytes()

    class ReplacedPath:
        """Simulate replacement after a stream is opened, deterministically."""
        def __init__(self):
            self.replaced = False
            self.reads = 0

        def open(self, mode):
            assert mode == "rb"
            self.replaced = True
            return io.BytesIO(original)

        def stat(self):
            return SimpleNamespace(st_size=len(original))

        def read_bytes(self):
            self.reads += 1
            return b"different ELF" if self.replaced or self.reads > 1 else original

    changing = ReplacedPath()
    checked = validator.validate(changing)
    if changing.reads != 1 or checked["sha256"] != hashlib.sha256(original).hexdigest():
        raise RuntimeError("validator did not parse and hash one immutable snapshot")
    print("PASS ELF snapshot consistency")
    with args.elf.open("rb") as stream:
        elf = ELFFile(stream)
        phdrs = [elf["e_phoff"] + i * elf["e_phentsize"] for i, seg in
                 enumerate(elf.iter_segments()) if seg["p_type"] == "PT_LOAD"]
        text = next(s for s in elf.iter_sections() if s["sh_flags"] & 4)
        instruction = text["sh_offset"]
        section_index = next(i for i, s in enumerate(elf.iter_sections()) if s["sh_flags"] & 4)
        section_header = elf["e_shoff"] + section_index * elf["e_shentsize"]
        section_size = text["sh_size"]
    # ELF32 header / program-header field offsets follow the ELF specification.
    cases = {
        "wrong-abi": ([(36, 1)], "single-float ABI"),
        "flash-entry": ([(24, 0x400000)], "entry must"),
        "flash-segment": ([(phdrs[0] + 8, 0x400000), (phdrs[0] + 12, 0x400000)], "outside owned RAM"),
        "data-relocation": ([(phdrs[0] + 12, 0x400000)], "relocation from flash"),
        "overlap": ([(phdrs[1] + 8, 0x28000), (phdrs[1] + 12, 0x28000)], "overlapping load"),
        "atomic": ([(instruction, 0x1000202F)], "atomic instruction"),
        "long-instruction": ([(instruction, 0x0000001F)], "unsupported long instruction"),
        "short-executable-load": ([(phdrs[0] + 16, 2)], "file-backed load segment"),
    }
    with tempfile.TemporaryDirectory(prefix="ws53-elf-guards-") as work:
        for name, (patches, expected) in cases.items():
            data = bytearray(original)
            for offset, value in patches:
                struct.pack_into("<I", data, offset, value)
            fixture = Path(work) / name
            fixture.write_bytes(data)
            try:
                validator.validate(fixture)
            except ValueError as error:
                if expected not in str(error):
                    raise RuntimeError(f"{name}: wrong rejection: {error}") from error
            else:
                raise RuntimeError(f"{name}: unsafe ELF accepted")
            # uv's current interpreter, with optimization enabled: guards remain.
            result = subprocess.run([sys.executable, "-O", str(CHECK), str(fixture)],
                                    capture_output=True, text=True, timeout=10)
            if result.returncode == 0 or expected not in result.stderr:
                raise RuntimeError(f"{name}: -O bypassed or changed guard: {result.stderr}")
            print(f"PASS {name} (normal and -O)")
        # Put forbidden instructions in the loaded bytes but point the section
        # to a safe duplicate appended after the ELF tables. Scanning only the
        # section without checking its file mapping would miss the instruction.
        data = bytearray(original)
        safe_copy = data[instruction:instruction + section_size]
        struct.pack_into("<I", data, section_header + 16, len(data))
        struct.pack_into("<I", data, instruction, 0x1000202F)
        data.extend(safe_copy)
        fixture = Path(work) / "aliased-executable-section"
        fixture.write_bytes(data)
        for optimized in (False, True):
            command = [sys.executable] + (["-O"] if optimized else []) + [str(CHECK), str(fixture)]
            result = subprocess.run(command, capture_output=True, text=True, timeout=10)
            if result.returncode == 0 or "file-backed load segment" not in result.stderr:
                raise RuntimeError(f"aliased section accepted or wrong rejection: {result.stderr}")
        print("PASS aliased-executable-section (normal and -O)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
