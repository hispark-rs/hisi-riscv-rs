#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Check WS63 HAL async-delay completion without Embassy or hardware access."""
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
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise FileExistsError(args.output)
    command = [str(args.qemu.resolve()), "-M", "ws63", "-display", "none",
               "-monitor", "none", "-serial", "stdio", "-kernel", str(args.elf.resolve()),
               "-d", "guest_errors"]
    proc = subprocess.Popen(command, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    timed_out = False
    try:
        stdout, stderr = proc.communicate(timeout=4)
    except subprocess.TimeoutExpired:
        timed_out = True
        proc.terminate()
        stdout, stderr = proc.communicate(timeout=3)
    lines = stderr.decode(errors="replace").splitlines()
    # Keep the existing baud warning in evidence: chardev is not a wire model.
    unexpected = [s for s in lines if not s.startswith("ws63-uart: baud divider set ")
                  and not re.fullmatch(
                      r"qemu-system-riscv32: terminating on signal 15 from pid \d+ .*", s)]
    ticks = re.findall(rb"async tick #(\d+)\r?\n", stdout)
    passed = (ticks == [b"1", b"2", b"3", b"4", b"5"]
              and stdout.count(b"ASYNC DELAY: PASS") == 1
              and b"FAIL" not in stdout and not unexpected
              and (proc.returncode == 0 or timed_out))
    result = dict(passed=passed, hardware_accessed=False, physical_timing_qualified=False,
                  race_injection=False, command=command, returncode=proc.returncode,
                  runner_terminated=timed_out,
                  elf_sha256=hashlib.sha256(args.elf.read_bytes()).hexdigest(),
                  qemu_sha256=hashlib.sha256(args.qemu.read_bytes()).hexdigest(),
                  stdout=stdout.decode(errors="replace"), stderr=lines,
                  unexpected_diagnostics=unexpected)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"passed": passed, "hardware_accessed": False}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
