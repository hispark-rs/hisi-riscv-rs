#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Check stale timer status against a second modeled timer, without hardware."""
import argparse
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--qemu", required=True, type=Path)
parser.add_argument("--elf", required=True, type=Path)
args = parser.parse_args()
process = subprocess.Popen([str(args.qemu.resolve()), "-M", "bs21", "-nographic",
                            "-serial", "mon:stdio", "-kernel", str(args.elf.resolve())],
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
try:
    stdout, stderr = process.communicate(timeout=4)
    raise RuntimeError(f"QEMU exited unexpectedly ({process.returncode}): {stderr!r}")
except subprocess.TimeoutExpired:
    process.kill()
    stdout, stderr = process.communicate(timeout=5)
if b"STALE OK" not in stdout or b"FAILED" in stdout:
    raise ValueError(f"timer regression failed: {stdout!r} {stderr!r}")
print("PASS BS21 modeled stale timer expiry")
