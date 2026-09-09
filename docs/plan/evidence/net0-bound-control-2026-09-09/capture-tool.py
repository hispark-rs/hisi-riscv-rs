#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyserial==3.5"]
# ///
"""Temporary manual NET0 control-plane capture, not a native queue-fence gate."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

p = argparse.ArgumentParser()
p.add_argument("--root", type=Path, required=True)
p.add_argument("--elf", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--source", required=True)
p.add_argument("--probe", required=True)
p.add_argument("--reset-serial", required=True)
p.add_argument("--port", required=True)
p.add_argument("--peer-port", required=True)
p.add_argument("--yaml", required=True)
p.add_argument("--role", choices=["ap", "sta"], required=True)
p.add_argument("--runs", type=int, default=3)
p.add_argument("--seconds", type=int, default=150)
p.add_argument("--flash", action="store_true")
p.add_argument("--require-bound-closed", action="store_true")
a = p.parse_args()
sys.path.insert(0, str(a.root / "hil"))
from jlink_nrst import pulse_nrst
import serial

a.output.mkdir(parents=True, exist_ok=False)
summary = {"schema": "net0-control-hil/v1", "source_commit": a.source,
           "elf_sha256": hashlib.sha256(a.elf.read_bytes()).hexdigest(),
           "role": a.role, "probe": a.probe, "runs": [],
           "boundary": "Native control-plane sequencing only; no L2 traffic or native producer-fence claim"}

def save():
    (a.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")

save()
if a.flash:
    env = dict(os.environ, METHOD="probe-rs", PROBE_SPEED="3000", PROBE_RS_PROBE=a.probe,
               PROBE_RS_YAML=a.yaml, WS63_RS=str(a.output))
    began = time.monotonic()
    with (a.output / "flash.log").open("wb") as log:
        result = subprocess.run(["bash", str(a.root / "hil/flash.sh"), str(a.elf)],
                                env=env, stdout=log, stderr=subprocess.STDOUT, timeout=360)
    summary["flash"] = {"exit_code": result.returncode, "seconds": time.monotonic() - began,
                        "verify_requested": True, "speed_khz": 3000}
    save()
    if result.returncode:
        print(json.dumps({"flash_failed": summary["flash"]}), flush=True)
        raise SystemExit(result.returncode)
    print(json.dumps({"flash": summary["flash"]}), flush=True)

required = [b"RFDBG_SOFTAP_READY"] if a.role == "ap" else [
    b"RFDBG_A5B_CONNECT_OK", b"RFDBG_A5B_DISCONNECT_OK", b"RFDBG_A5B_CONNECT_PROFILE_OK"]
if a.require_bound_closed:
    if a.role != "sta":
        raise SystemExit("The bound/closed fixture is a station-only contract")
    required.append(b"RFDBG_NET0_BOUND_CLOSED")
summary["required_markers"] = [marker.decode() for marker in required]
save()
for index in range(1, a.runs + 1):
    with contextlib.ExitStack() as stack:
        ports = [stack.enter_context(serial.Serial(name, 115200, timeout=0))
                 for name in [a.port, a.peer_port]]
        for port in ports:
            port.reset_input_buffer()
        pulse_nrst("JLinkExe", a.reset_serial)
        buffers = [bytearray(), bytearray()]
        began = time.monotonic()
        observed = None
        while time.monotonic() - began < a.seconds:
            for port, data in zip(ports, buffers):
                data.extend(port.read(4096))
                if len(data) > 2 * 1024 * 1024:
                    raise RuntimeError("UART capture exceeded its fixed bound")
            if all(marker in buffers[0] for marker in required):
                observed = observed or time.monotonic()
            if observed is not None and time.monotonic() - observed > 1:
                break
            time.sleep(0.01)
    captures = []
    for suffix, data in zip(["target", "peer"], buffers):
        log = a.output / f"run-{index:02}.{suffix}.uart.log"
        log.write_bytes(data)
        captures.append({"role": suffix, "bytes": len(data),
                         "sha256": hashlib.sha256(data).hexdigest()})
    target = buffers[0]
    failed = any(marker in target for marker in [b"panicked at", b"PANIC", b"RFDBG_A5B_CONNECT_ERR",
                 b"RFDBG_A5B_DISCONNECT_ERR", b"RFDBG_A5B_BOOTSTRAP_ERR"])
    run = {"index": index, "pass": not failed and all(marker in target for marker in required),
           "elapsed_seconds": time.monotonic() - began,
           "markers": {marker.decode(): marker in target for marker in required}, "captures": captures}
    summary["runs"].append(run)
    save()
    print(json.dumps(run), flush=True)
    if not run["pass"]:
        break
raise SystemExit(0 if len(summary["runs"]) == a.runs and all(x["pass"] for x in summary["runs"]) else 1)
