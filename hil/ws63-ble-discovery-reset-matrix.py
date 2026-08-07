#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyserial"]
# ///
"""Run a paired-board WS63 BLE advertising/scanning reset matrix."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import serial

from jlink_nrst import pulse_nrst


REQUIRED_BOOT_MARKERS = (
    b"RFDBG_BLE_B1_INIT_OK",
    b"RFDBG_BLE_B2_COMMANDS_OK",
    b"RFDBG_BLE_B2_SCAN_READY",
    b"RFDBG_BLE_B2_ADV_OK",
)
MATCH_MARKER = b"RFDBG_BLE_B2_SCAN_MATCH"
FAILURE_MARKERS = (
    b"RFDBG_MISSING_ROM_CALLBACK",
    b"RFDBG_BLE_B2_COMMAND_ERR",
    b"RFDBG_BLE_B2_EVENT_DROP",
    b"RFDBG_BLE_B2_EVENT_ERR",
)


def drain(port: serial.Serial) -> bytes:
    waiting = port.in_waiting
    return port.read(waiting) if waiting else b""


def classify(target: bytes, peer: bytes) -> dict[str, object]:
    target_missing = [
        marker.decode() for marker in REQUIRED_BOOT_MARKERS if marker not in target
    ]
    peer_missing = [
        marker.decode() for marker in REQUIRED_BOOT_MARKERS if marker not in peer
    ]
    failures = [
        marker.decode()
        for marker in FAILURE_MARKERS
        if marker in target or marker in peer
    ]
    target_match = MATCH_MARKER in target
    peer_match = MATCH_MARKER in peer
    passed = not target_missing and not peer_missing and not failures and target_match
    return {
        "pass": passed,
        "target_missing": target_missing,
        "peer_missing": peer_missing,
        "failure_markers": failures,
        "target_match": target_match,
        "peer_match": peer_match,
        "target_bytes": len(target),
        "peer_bytes": len(peer),
    }


def capture_run(
    target: serial.Serial,
    peer: serial.Serial,
    target_jlink: str,
    peer_jlink: str,
    peer_settle: float,
    timeout: float,
) -> tuple[bytes, bytes]:
    target.reset_input_buffer()
    peer.reset_input_buffer()

    pulse_nrst("JLinkExe", peer_jlink)
    peer_log = bytearray()
    settle_deadline = time.monotonic() + peer_settle
    while time.monotonic() < settle_deadline:
        peer_log.extend(drain(peer))
        time.sleep(0.01)

    pulse_nrst("JLinkExe", target_jlink)
    target_log = bytearray()
    deadline = time.monotonic() + timeout
    matched_at: float | None = None
    while time.monotonic() < deadline:
        target_log.extend(drain(target))
        peer_log.extend(drain(peer))
        if MATCH_MARKER in target_log and matched_at is None:
            matched_at = time.monotonic()
        if matched_at is not None and time.monotonic() - matched_at >= 1.0:
            break
        time.sleep(0.01)

    target_log.extend(drain(target))
    peer_log.extend(drain(peer))
    return bytes(target_log), bytes(peer_log)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target-port", required=True)
    parser.add_argument("--target-jlink", required=True)
    parser.add_argument("--peer-port", required=True)
    parser.add_argument("--peer-jlink", required=True)
    parser.add_argument("--baud", type=int, default=115_200)
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--peer-settle", type=float, default=3.0)
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.runs <= 0 or args.peer_settle <= 0 or args.timeout <= 0:
        raise SystemExit("--runs, --peer-settle and --timeout must be positive")
    args.output.mkdir(parents=True, exist_ok=False)

    records: list[dict[str, object]] = []
    with serial.Serial(args.target_port, args.baud, timeout=0) as target:
        with serial.Serial(args.peer_port, args.baud, timeout=0) as peer:
            for run in range(1, args.runs + 1):
                target_log, peer_log = capture_run(
                    target,
                    peer,
                    args.target_jlink,
                    args.peer_jlink,
                    args.peer_settle,
                    args.timeout,
                )
                target_name = f"run-{run:02d}.target.uart.log"
                peer_name = f"run-{run:02d}.peer.uart.log"
                (args.output / target_name).write_bytes(target_log)
                (args.output / peer_name).write_bytes(peer_log)
                record = classify(target_log, peer_log)
                record.update(
                    {"run": run, "target_log": target_name, "peer_log": peer_name}
                )
                records.append(record)
                state = "pass" if record["pass"] else "fail"
                print(f"run {run:02d}/{args.runs}: {state}", flush=True)

    passed = sum(record["pass"] is True for record in records)
    summary = {
        "schema_version": 1,
        "runs": args.runs,
        "passed": passed,
        "failed": args.runs - passed,
        "target_port": args.target_port,
        "target_jlink": args.target_jlink,
        "peer_port": args.peer_port,
        "peer_jlink": args.peer_jlink,
        "records": records,
    }
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"summary: {passed}/{args.runs} pass")
    print(f"artifacts: {args.output}")
    return 0 if passed == args.runs else 1


if __name__ == "__main__":
    raise SystemExit(main())
