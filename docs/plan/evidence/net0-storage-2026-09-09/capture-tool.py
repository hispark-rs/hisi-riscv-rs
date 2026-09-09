#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial==3.5"]
# ///
"""Temporary, credential-free NET0 bootstrap capture using repository reset code."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument("--root", type=Path, required=True)
parser.add_argument("--elf", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--probe", required=True)
parser.add_argument("--serial", required=True)
parser.add_argument("--ports", nargs="+", required=True)
parser.add_argument("--yaml", required=True)
parser.add_argument("--runs", type=int, default=3)
parser.add_argument("--seconds", type=int, default=25)
parser.add_argument("--flash", action="store_true")
args = parser.parse_args()
sys.path.insert(0, str(args.root / "hil"))
from jlink_nrst import pulse_nrst
import serial

args.output.mkdir(parents=True, exist_ok=True)
summary = {"schema": "net0-bootstrap-hil/v1", "source_commit": "e117f585b80d72779fc3c6fb26b8bbd9564a918f",
           "elf_sha256": hashlib.sha256(args.elf.read_bytes()).hexdigest(),
           "probe": args.probe, "runs": [], "boundary": "Bootstrap resource admission only; no L2 session/traffic/fence claim"}
if args.flash:
    env = dict(os.environ, METHOD="probe-rs", PROBE_SPEED="3000", PROBE_RS_PROBE=args.probe,
               PROBE_RS_YAML=args.yaml)
    began = time.monotonic()
    with (args.output / "flash.log").open("wb") as log:
        result = subprocess.run(["bash", str(args.root / "hil/flash.sh"), str(args.elf)],
                                env=env, stdout=log, stderr=subprocess.STDOUT, timeout=300)
    summary["flash"] = {"exit_code": result.returncode, "seconds": time.monotonic() - began,
                        "verify": True, "speed_khz": 3000}
    if result.returncode:
        (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps(summary))
        raise SystemExit(result.returncode)

for index in range(1, args.runs + 1):
    with contextlib.ExitStack() as stack:
        ports = [stack.enter_context(serial.Serial(port, 115200, timeout=0)) for port in args.ports]
        for port in ports:
            port.reset_input_buffer()
        pulse_nrst("JLinkExe", args.serial)
        buffers = [bytearray() for _ in ports]
        deadline = time.monotonic() + args.seconds
        while time.monotonic() < deadline:
            for port, data in zip(ports, buffers):
                data.extend(port.read(4096))
                if len(data) > 1024 * 1024:
                    raise RuntimeError("bounded UART capture capacity exceeded")
            time.sleep(0.01)
    captures = []
    for name, data in zip(args.ports, buffers):
        log = args.output / f"run-{index:02}-{Path(name).name}.uart.log"
        log.write_bytes(data)
        captures.append({"port": name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                         "bootstrap_ok": b"RFDBG_BOOTSTRAP_PROFILE_OK" in data,
                         "stack_admission_ok": b"A5U_TASK_STACK_ADMISSION_OK" in data,
                         "bootstrap_error": b"RFDBG_BOOTSTRAP_PROFILE_ERR" in data,
                         "panic": b"panicked at" in data or b"PANIC" in data})
    passed = [item for item in captures if item["bootstrap_ok"] and item["stack_admission_ok"] and
              not item["bootstrap_error"] and not item["panic"]]
    run = {"index": index, "pass": len(passed) == 1, "captures": captures}
    summary["runs"].append(run)
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(run), flush=True)
    if not run["pass"]:
        break
raise SystemExit(0 if len(summary["runs"]) == args.runs and all(r["pass"] for r in summary["runs"]) else 1)
