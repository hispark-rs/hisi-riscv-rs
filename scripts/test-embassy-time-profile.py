#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Run one RTOS Embassy profile in QEMU; never probe, reset or flash hardware."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qemu", type=Path, required=True)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--case", choices=("ws63", "bs21", "ws53", "coexist"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise FileExistsError(args.output)
    machine = "ws63" if args.case == "coexist" else args.case
    marker = {"ws63": b"EMBASSY MULTITASK: PASS", "bs21": b"BS21 EMBASSY MULTITASK: PASS",
              "ws53": b"WS53 PASS embassy irq=54 timers=5 ticker=3 guards=3 cancel=1 capacity=16 reenter=1 monotonic=1",
              "coexist": b"A3_RTOS_EMBASSY_COEXIST_OK"}[args.case]
    command = [str(args.qemu.resolve()), "-M", machine, "-display", "none", "-monitor", "none",
               "-serial", "stdio", "-kernel", str(args.elf.resolve()), "-d", "guest_errors"]
    proc = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        stdout, stderr = proc.communicate(timeout=4)
    except subprocess.TimeoutExpired:
        proc.terminate()
        stdout, stderr = proc.communicate(timeout=3)
    lines = stderr.decode(errors="replace").splitlines()
    # The existing UART model warns about the boot clock baud selection. Preserve
    # that diagnostic; it is not physical baud qualification.
    unexpected = [s for s in lines if not re.fullmatch(
        r"qemu-system-riscv32: terminating on signal 15 from pid \d+ .*", s)
        and not s.startswith("ws63-uart: baud divider set ")]
    passed = marker in stdout and b"FAIL" not in stdout and not unexpected
    result = dict(case=args.case, passed=passed, hardware_accessed=False,
                  physical_timing_qualified=False, command=command,
                  elf_sha256=hashlib.sha256(args.elf.read_bytes()).hexdigest(),
                  qemu_sha256=hashlib.sha256(args.qemu.read_bytes()).hexdigest(),
                  stdout=stdout.decode(errors="replace"), stderr=lines,
                  unexpected_diagnostics=unexpected)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: result[k] for k in ("case", "passed", "hardware_accessed")}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
