#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Check HAL timer IRQ waits without Embassy or hardware access."""
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
    parser.add_argument("--chip", choices=("ws63", "ws53"), default="ws63")
    parser.add_argument("--lifecycle", action="store_true", help="Require WS63 opt-in lifecycle checks")
    parser.add_argument("--channel", type=int, choices=(0, 1, 2), default=0,
                        help="WS63 lifecycle channel compiled into the fixture")
    args = parser.parse_args()
    if args.output.exists():
        raise FileExistsError(args.output)
    serial = ["-serial", "null", "-serial", "null"] if args.chip == "ws53" else []
    command = [str(args.qemu.resolve()), "-M", args.chip, "-display", "none",
               "-monitor", "none", *serial, "-serial", "stdio", "-kernel", str(args.elf.resolve()),
               "-icount", "shift=0,sleep=on", "-d", "guest_errors"]
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
    if args.chip == "ws53":
        transcript_ok = (stdout.count(b"WS53 PASS timer_async channels=2 expiries=16 cancels=4 guards=6") == 1
                         and b"WS53 PASS timer_irq=53 count=3" in stdout)
    else:
        transcript_ok = (ticks == [b"1", b"2", b"3", b"4", b"5"]
                         and stdout.count(b"ASYNC DELAY: PASS") == 1)
    if args.lifecycle:
        transcript_ok = transcript_ok and args.chip == "ws63" and stdout.count(
            b"TIMER LIFECYCLE: PASS cancel=4 zero=1 unpolled=1 spurious=1 rounding=1 long=1") == 1
        transcript_ok = transcript_ok and stdout.count(f"TIMER CHANNEL: {args.channel}\r\n".encode()) == 1
    passed = (transcript_ok
              and b"FAIL" not in stdout and not unexpected
              and (proc.returncode == 0 or timed_out))
    result = dict(passed=passed, hardware_accessed=False, physical_timing_qualified=False,
                  race_injection=False, chip=args.chip, lifecycle=args.lifecycle, channel=args.channel,
                  command=command, returncode=proc.returncode,
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
