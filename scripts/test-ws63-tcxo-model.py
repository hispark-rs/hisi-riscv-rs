#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Run the bounded WS63 TCXO smoke in QEMU only; never access hardware."""
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
process = subprocess.Popen([str(qemu), '-M', 'ws63', '-display', 'none', '-monitor', 'none',
    '-serial', 'stdio', '-kernel', str(elf), '-d', 'guest_errors'], stdin=subprocess.DEVNULL,
    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
try:
    stdout, stderr = process.communicate(timeout=5)
    raise RuntimeError(f'fixture unexpectedly exited: {process.returncode}, {stderr!r}')
except subprocess.TimeoutExpired:
    process.terminate()
    stdout, stderr = process.communicate(timeout=5)
diagnostics = stderr.decode(errors='replace').splitlines()
passed = stdout == b'WS63 TCXO EMBASSY PASS\n' and all(
    re.fullmatch(r'qemu-system-riscv32: terminating on signal 15 from pid \d+ .*', line)
    for line in diagnostics)
result = dict(passed=passed, hardware_accessed=False, long_uptime_qualified=False,
    elf_sha256=hashlib.sha256(elf.read_bytes()).hexdigest(),
    qemu_sha256=hashlib.sha256(qemu.read_bytes()).hexdigest(),
    stdout=stdout.decode(errors='replace'), stderr=stderr.decode(errors='replace'))
output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
if not passed:
    raise SystemExit(1)
