#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Run the WS63/BS21 SPI write-drain examples against a supplied QEMU.

The example firmware intentionally loops forever. A successful check observes
both serial markers before terminating the emulator; it does not test silicon,
physical timing or the capacity of the actual hardware FIFOs.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def run(qemu: Path, machine: str, elf: Path) -> dict:
    command = [str(qemu), "-M", machine, "-nographic", "-serial", "mon:stdio", "-kernel", str(elf)]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        stdout, stderr = process.communicate(timeout=4)
        raise RuntimeError(f"{machine} emulator exited unexpectedly ({process.returncode}): {stderr!r}")
    except subprocess.TimeoutExpired:
        process.kill()
        stdout, stderr = process.communicate(timeout=5)
    if b"SPI loopback OK" not in stdout or b"SPI write drain OK" not in stdout:
        raise ValueError(f"{machine} SPI check failed: {stdout!r} {stderr!r}")
    if b"FAILED" in stdout or b"MISMATCH" in stdout or b"SPI error" in stdout:
        raise ValueError(f"{machine} reported a SPI failure: {stdout!r}")
    return {"machine": machine, "elf_sha256": hashlib.sha256(elf.read_bytes()).hexdigest(),
            "uart": stdout.decode("utf-8", errors="replace")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("qemu", "ws63-elf", "bs21-elf", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    results = [run(args.qemu.resolve(), "ws63", args.ws63_elf.resolve()),
               run(args.qemu.resolve(), "bs21", args.bs21_elf.resolve())]
    report = {"passed": True, "hardware_accessed": False,
              "qemu_sha256": hashlib.sha256(args.qemu.read_bytes()).hexdigest(),
              "scope": "synthetic loopback; no silicon FIFO or timing qualification", "cases": results}
    with args.output.open("x") as file:
        json.dump(report, file, indent=2)
        file.write("\n")
    print("PASS WS63/BS21 SPI write drain")


if __name__ == "__main__":
    main()
