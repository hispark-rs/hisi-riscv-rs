#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Run GPIO async lifecycle fixtures on QEMU only, without probe/serial access."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--chip", choices=("ws63", "ws53"), required=True)
    p.add_argument("--qemu", type=Path, required=True)
    p.add_argument("--elf", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    if a.output.exists():
        raise FileExistsError(a.output)
    serial = ["-serial", "null", "-serial", "null"] if a.chip == "ws53" else []
    command = [str(a.qemu.resolve()), "-M", a.chip, "-display", "none", "-monitor", "none",
               *serial, "-serial", "stdio", "-kernel", str(a.elf.resolve()),
               "-icount", "shift=0,sleep=on", "-d", "guest_errors"]
    proc = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    terminated = False
    try:
        out, err = proc.communicate(timeout=4)
    except subprocess.TimeoutExpired:
        terminated = True
        proc.terminate()
        out, err = proc.communicate(timeout=3)
    marker = (b"GPIO ASYNC: PASS simultaneous=2 spurious=1 cancels=3 immediate=2 edges=3"
              if a.chip == "ws63" else
              b"WS53 PASS gpio_async pad=17 irq=34 wakes=8 immediate=2 cancels=2 guards=2 restore=1")
    lines = err.decode(errors="replace").splitlines()
    unexpected = [line for line in lines if not line.startswith("ws63-uart: baud divider set ")
                  and not re.fullmatch(r"qemu-system-riscv32: terminating on signal 15 from pid \d+ .*", line)]
    passed = (out.count(marker) == 1 and b"FAIL" not in out and not unexpected
              and (proc.returncode == 0 or terminated))
    result = dict(passed=passed, hardware_accessed=False, physical_timing_qualified=False,
                  chip=a.chip, command=command, runner_terminated=terminated,
                  returncode=proc.returncode, stdout=out.decode(errors="replace"), stderr=lines,
                  unexpected_diagnostics=unexpected,
                  elf_sha256=hashlib.sha256(a.elf.read_bytes()).hexdigest(),
                  qemu_sha256=hashlib.sha256(a.qemu.read_bytes()).hexdigest())
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
