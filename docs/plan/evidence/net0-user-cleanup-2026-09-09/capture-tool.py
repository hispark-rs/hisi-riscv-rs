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
import re
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
p.add_argument("--require-host-delivery", action="store_true")
p.add_argument("--initial-session-experiment", action="store_true")
p.add_argument("--expect-cleanup-fault", action="store_true")
p.add_argument("--reset-peer", help="Reset the AP before every STA round and require its ready marker")
a = p.parse_args()
sys.path.insert(0, str(a.root / "hil"))
from jlink_nrst import pulse_nrst
import serial

a.output.mkdir(parents=True, exist_ok=False)
summary = {"schema": "net0-control-hil/v1", "source_commit": a.source,
           "elf_sha256": hashlib.sha256(a.elf.read_bytes()).hexdigest(),
           "role": a.role, "probe": a.probe, "runs": [],
           "boundary": "Native control-plane sequencing only; no L2 traffic or native producer-fence claim"}
if a.reset_peer:
    if a.role != "sta":
        raise SystemExit("--reset-peer requires STA role")
    summary["reset_policy"] = {"kind": "paired-ap-then-sta", "peer_serial": a.reset_peer}
else:
    summary["reset_policy"] = {"kind": "target-only"}

def save():
    (a.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")

def host_delivery(data):
    fields = ("entered", "returned", "abandoned", "in_flight", "peak", "closed",
              "crossed_close", "nonzero")
    pattern = (rb"RFDBG_NET0_HOST_DELIVERY phase=(bootstrap|connected|disconnected)"
               + b"".join(b" " + key.encode() + rb"=0x([0-9a-fA-F]{16})" for key in fields)
               + rb" exhausted=([01])\r?\n")
    samples = []
    errors = []
    previous = 0
    for match in re.finditer(pattern, data):
        sample = dict(zip(fields, (int(value, 16) for value in match.groups()[1:-1])))
        sample["phase"] = match[1].decode()
        sample["exhausted"] = match.groups()[-1] == b"1"
        if sample["exhausted"] or sample["entered"] != (sample["returned"] + sample["abandoned"] + sample["in_flight"]):
            errors.append("call_conservation")
        if sample["abandoned"] or sample["nonzero"] or sample["in_flight"]:
            errors.append("call_not_cleanly_retired_at_sample")
        if sample["closed"] != sample["entered"] or sample["entered"] < previous:
            errors.append("closed_route_or_counter_regression")
        previous = sample["entered"]
        samples.append(sample)
    if [item["phase"] for item in samples] != ["bootstrap", "connected", "disconnected"]:
        errors.append("missing_duplicate_or_reordered_phase")
    if not samples or samples[-1]["entered"] == 0:
        errors.append("native_hook_not_observed")
    return {"samples": samples, "errors": errors, "pass": not errors,
            "boundary": "Call-lifetime observations only; not native queue drainage or Ethernet delivery"}

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
if a.initial_session_experiment:
    if a.role != "sta" or a.require_host_delivery:
        raise SystemExit("Initial session is a separate STA payload contract, not closed-route evidence")
    required += [b"RFDBG_NET0_INITIAL_SESSION_OPEN", b"RFDBG_NET0_PAYLOAD_OK",
                 b"RFDBG_NET0_INITIAL_SESSION_CLOSED"]
    summary["schema"] = "net0-initial-session-hil/v1"
    summary["boundary"] = "One-shot initial-session L2/ARP/UDP experiment; no native drain/reconnect proof"
if a.require_bound_closed:
    if a.role != "sta":
        raise SystemExit("The bound/closed fixture is a station-only contract")
    required.append(b"RFDBG_NET0_BOUND_CLOSED")
if a.expect_cleanup_fault:
    if not a.initial_session_experiment:
        raise SystemExit("Cleanup fault requires the initial-session fixture")
    required = [marker for marker in required if marker not in [
        b"RFDBG_A5B_DISCONNECT_OK", b"RFDBG_A5B_CONNECT_PROFILE_OK",
        b"RFDBG_NET0_INITIAL_SESSION_CLOSED"]]
    required.append(b"RFDBG_NET0_CLEANUP_FAULT_REJECTED")
    summary["schema"] = "net0-cleanup-fault-hil/v1"
    summary["boundary"] = "Negative return-status injection after real resource release; not a normal connectivity success or native drain proof"
summary["required_markers"] = [marker.decode() for marker in required]
save()
for index in range(1, a.runs + 1):
    with contextlib.ExitStack() as stack:
        ports = [stack.enter_context(serial.Serial(name, 115200, timeout=0))
                 for name in [a.port, a.peer_port]]
        for port in ports:
            port.reset_input_buffer()
        buffers = [bytearray(), bytearray()]
        peer_reset_seconds = None
        if a.reset_peer:
            peer_began = time.monotonic()
            pulse_nrst("JLinkExe", a.reset_peer)
            while b"RFDBG_SOFTAP_READY" not in buffers[1]:
                buffers[1].extend(ports[1].read(4096))
                if time.monotonic() - peer_began > 45:
                    (a.output / f"run-{index:02}.peer-start-failed.uart.log").write_bytes(buffers[1])
                    summary["peer_start_failure"] = {"index": index, "reason": "ready_timeout"}
                    save()
                    raise SystemExit("Paired AP did not report ready")
                time.sleep(0.01)
            peer_reset_seconds = time.monotonic() - peer_began
        ports[0].reset_input_buffer()
        pulse_nrst("JLinkExe", a.reset_serial)
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
    failures = [b"panicked at", b"PANIC", b"RFDBG_A5B_CONNECT_ERR",
                 b"RFDBG_A5B_BOOTSTRAP_ERR", b"RFDBG_NET0_PAYLOAD_ERR",
                 b"RFDBG_NET0_INITIAL_SESSION_REJECTED"]
    if not a.expect_cleanup_fault:
        failures.append(b"RFDBG_A5B_DISCONNECT_ERR")
    failed = any(marker in target for marker in failures)
    run = {"index": index, "pass": not failed and all(marker in target for marker in required),
           "elapsed_seconds": time.monotonic() - began,
           "markers": {marker.decode(): marker in target for marker in required}, "captures": captures}
    if peer_reset_seconds is not None:
        run["peer_reset_seconds"] = peer_reset_seconds
    if a.require_host_delivery:
        run["host_delivery"] = host_delivery(target)
        run["pass"] &= run["host_delivery"]["pass"]
    if a.initial_session_experiment:
        payloads = re.findall(rb"RFDBG_NET0_PAYLOAD sent=0x([0-9a-fA-F]{8}) received=0x([0-9a-fA-F]{8}) bitmap=0x([0-9a-fA-F]{8}) invalid=0x([0-9a-fA-F]{8}) duplicate=0x([0-9a-fA-F]{8})", target)
        run["payload"] = dict(zip(["sent", "received", "bitmap", "invalid", "duplicate"],
                                   [int(v, 16) for v in payloads[0]])) if len(payloads) == 1 else None
        run["pass"] &= (run["payload"] is not None and run["payload"]["sent"] == 10
                        and run["payload"]["received"] == 10 and run["payload"]["bitmap"] == 1023
                        and run["payload"]["invalid"] == 0)
        samples = re.findall(rb"(?m)^RFDBG_NET0_USER_CLEANUP" + rb" 0x([0-9a-fA-F]{8})" * 10 + rb"\r?\n", target)
        decoded = [[int(value, 16) for value in sample] for sample in samples]
        code = 100 if a.expect_cleanup_fault else 0
        expected = [1, 1, 0, 1, 0, code, 0, 1, code, code]
        run["cleanup"] = {"samples": decoded, "pass": decoded == [[0] * 10, expected]}
        run["pass"] &= run["cleanup"]["pass"]
    if a.expect_cleanup_fault:
        errors = re.findall(rb"(?m)^RFDBG_A5B_DISCONNECT_ERR code=[a-z0-9_.-]+ stage=[a-z0-9_.-]+ backend=0x([0-9a-fA-F]{8})\r?\n", target)
        forbidden = [b"RFDBG_A5B_DISCONNECT_OK", b"RFDBG_A5B_CONNECT_PROFILE_OK", b"RFDBG_NET0_INITIAL_SESSION_CLOSED"]
        run["expected_error"] = {"backend_codes": [int(code, 16) for code in errors],
                                 "false_success_markers": [marker.decode() for marker in forbidden if marker in target]}
        raw = re.findall(rb"(?m)^RFDBG_A5B_DISCONNECT_ERR_TRACE kind=hostap_status value=0x([0-9a-fA-F]{8})\r?\n", target)
        run["expected_error"]["native_status"] = [int(code, 16) for code in raw]
        run["pass"] &= errors == [b"5732d064"] and raw == [b"00000064"] and not run["expected_error"]["false_success_markers"]
    summary["runs"].append(run)
    save()
    print(json.dumps(run), flush=True)
    if not run["pass"]:
        break
raise SystemExit(0 if len(summary["runs"]) == a.runs and all(x["pass"] for x in summary["runs"]) else 1)
