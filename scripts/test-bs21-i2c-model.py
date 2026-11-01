#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Run the BS21 I2C virtual-peer smoke; never access physical hardware."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
for name in ('qemu', 'elf', 'output'):
    parser.add_argument('--' + name, type=Path, required=True)
args = parser.parse_args()
qemu, elf, output = (getattr(args, name).resolve() for name in ('qemu', 'elf', 'output'))
if output.exists():
    raise FileExistsError(output)
process = subprocess.Popen([str(qemu), '-M', 'bs21', '-display', 'none', '-monitor', 'none',
    '-serial', 'stdio', '-kernel', str(elf), '-d', 'guest_errors'], stdin=subprocess.DEVNULL,
    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
try:
    stdout, stderr = process.communicate(timeout=5)
    raise RuntimeError(f'fixture unexpectedly exited: {process.returncode}, {stderr!r}')
except subprocess.TimeoutExpired:
    process.terminate()
    stdout, stderr = process.communicate(timeout=5)
lines = stderr.decode(errors='replace').splitlines()
divider = re.compile(r'ws63-uart: baud divider set \(int=17 frac=(?:0|23)\) -> effective baud ~= (?:588235|576057) at the nominal 160000000 Hz UART clock\..*')
termination = re.compile(r'qemu-system-riscv32: terminating on signal 15 from pid \d+ .*')
passed = (stdout == b'BS21 I2C MODEL PASS\r\n' and sum(bool(divider.fullmatch(line)) for line in lines) == 3
          and all(divider.fullmatch(line) or termination.fullmatch(line) for line in lines))
result = dict(passed=passed, hardware_accessed=False, wire_protocol_qualified=False,
    arbitrary_abort_or_recovery_qualified=False,
    elf_sha256=hashlib.sha256(elf.read_bytes()).hexdigest(),
    qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(),
    stdout=stdout.decode(errors='replace'), stderr=stderr.decode(errors='replace'))
output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
if not passed:
    raise SystemExit(1)
